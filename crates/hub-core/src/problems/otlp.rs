//! OpenTelemetry in: a program that speaks OTLP over HTTP with JSON
//! (`OTEL_EXPORTER_OTLP_PROTOCOL=http/json`) sends to `/api/<source>/otlp/v1/logs` and
//! `/api/<source>/otlp/v1/traces`, with the source's key as `Authorization: Bearer <key>` or
//! `?key=`. Log records at warning and above become problems (with `exception.*` attributes as
//! the error and its stack); spans become timings by name, and a span that ended in an error
//! is a problem too.

use std::sync::Arc;

use anyhow::Result;
use serde_json::{Value, json};

use super::{Frame, Hit, Incoming, Level, Source};
use crate::hub::Hub;

/// An attribute list (`[{key, value: {stringValue | intValue | ...}}]`) as a JSON object.
fn attributes(list: &Value) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    for a in list.as_array().into_iter().flatten() {
        let Some(key) = a["key"].as_str() else { continue };
        out.insert(key.to_owned(), any_value(&a["value"]));
    }
    out
}

fn any_value(v: &Value) -> Value {
    for k in ["stringValue", "boolValue", "doubleValue"] {
        if let Some(x) = v.get(k) {
            return x.clone();
        }
    }
    if let Some(x) = v.get("intValue") {
        // OTLP JSON writes 64-bit integers as strings.
        return x.as_str().and_then(|s| s.parse::<i64>().ok()).map(Value::from).unwrap_or_else(|| x.clone());
    }
    if let Some(list) = v["arrayValue"]["values"].as_array() {
        return Value::Array(list.iter().map(any_value).collect());
    }
    if v.get("kvlistValue").is_some() {
        return Value::Object(attributes(&v["kvlistValue"]["values"]));
    }
    Value::Null
}

fn nanos_ms(v: &Value) -> Option<i64> {
    let n = v.as_str().and_then(|s| s.parse::<i128>().ok()).or_else(|| v.as_i64().map(i128::from))?;
    (n > 0).then_some((n / 1_000_000) as i64)
}

fn level(number: i64, text: Option<&str>) -> Level {
    if number >= 17 {
        Level::Error
    } else if number >= 13 {
        Level::Warning
    } else if number == 0 {
        Level::parse(text.unwrap_or("info"))
    } else {
        Level::Info
    }
}

/// `/v1/logs`: returns how many records became problems.
pub async fn logs(hub: &Arc<Hub>, source: &Source, body: &Value) -> Result<usize> {
    let mut kept = 0;
    for rl in body["resourceLogs"].as_array().into_iter().flatten() {
        let resource = attributes(&rl["resource"]["attributes"]);
        let release = resource.get("service.version").and_then(Value::as_str).map(str::to_owned);
        let environment = resource.get("deployment.environment").and_then(Value::as_str).map(str::to_owned);
        for sl in rl["scopeLogs"].as_array().into_iter().flatten() {
            let scope = sl["scope"]["name"].as_str().unwrap_or_default().to_owned();
            for r in sl["logRecords"].as_array().into_iter().flatten() {
                let level = level(r["severityNumber"].as_i64().unwrap_or(0), r["severityText"].as_str());
                if level < Level::Warning {
                    continue;
                }
                let attrs = attributes(&r["attributes"]);
                let body = match any_value(&r["body"]) {
                    Value::String(s) => s,
                    Value::Null => String::new(),
                    other => other.to_string(),
                };
                let text = |k: &str| attrs.get(k).and_then(Value::as_str).map(str::to_owned);
                let message = text("exception.message").filter(|_| body.is_empty()).unwrap_or(body);
                let file = text("code.filepath").or_else(|| text("code.file.path"));
                let function = text("code.function").or_else(|| text("code.function.name"));
                let line = attrs.get("code.lineno").or_else(|| attrs.get("code.line.number")).and_then(Value::as_u64).map(|l| l as u32);
                let frames = if file.is_some() || function.is_some() {
                    vec![Frame { file, function: function.or_else(|| Some(scope.clone()).filter(|s| !s.is_empty())), line, code: None, own: true }]
                } else {
                    Vec::new()
                };
                let incoming = Incoming {
                    project: source.name.clone(),
                    source: "otlp".into(),
                    level,
                    folder: source.folder.clone(),
                    fingerprint: None,
                    hit: Hit {
                        ts: nanos_ms(&r["timeUnixNano"]).or_else(|| nanos_ms(&r["observedTimeUnixNano"])).unwrap_or(0),
                        message,
                        kind: text("exception.type"),
                        frames,
                        trace: text("exception.stacktrace"),
                        crumbs: Vec::new(),
                        release: release.clone(),
                        environment: environment.clone(),
                        extra: json!({ "attributes": attrs, "resource": resource, "scope": scope, "trace_id": r["traceId"] }),
                    },
                };
                super::record(hub, incoming).await?;
                kept += 1;
            }
        }
    }
    Ok(kept)
}

/// `/v1/traces`: each span's time goes to the metrics; a span with an error status is a problem.
pub async fn traces(hub: &Arc<Hub>, source: &Source, body: &Value) -> Result<usize> {
    let mut kept = 0;
    for rs in body["resourceSpans"].as_array().into_iter().flatten() {
        let resource = attributes(&rs["resource"]["attributes"]);
        let release = resource.get("service.version").and_then(Value::as_str).map(str::to_owned);
        for ss in rs["scopeSpans"].as_array().into_iter().flatten() {
            for span in ss["spans"].as_array().into_iter().flatten() {
                let name = span["name"].as_str().unwrap_or("span");
                let (Some(start), Some(end)) = (nanos_ms(&span["startTimeUnixNano"]), nanos_ms(&span["endTimeUnixNano"])) else { continue };
                // Only spans that start a trace or come in from outside: the inner ones are parts of them.
                let root = span["parentSpanId"].as_str().is_none_or(str::is_empty) || span["kind"].as_i64() == Some(2);
                if root {
                    super::metrics::sample(&format!("tx:{name}"), (end - start) as f64, release.as_deref(), &source.name);
                }
                // Status code 2 is ERROR.
                if span["status"]["code"].as_i64() == Some(2) {
                    let attrs = attributes(&span["attributes"]);
                    let events: Vec<serde_json::Map<String, Value>> =
                        span["events"].as_array().into_iter().flatten().filter(|e| e["name"] == "exception").map(|e| attributes(&e["attributes"])).collect();
                    let ex = events.first();
                    let text = |k: &str| ex.and_then(|e| e.get(k)).and_then(Value::as_str).map(str::to_owned);
                    let incoming = Incoming {
                        project: source.name.clone(),
                        source: "otlp".into(),
                        level: Level::Error,
                        folder: source.folder.clone(),
                        fingerprint: None,
                        hit: Hit {
                            ts: end,
                            message: text("exception.message").or_else(|| span["status"]["message"].as_str().map(str::to_owned)).unwrap_or_else(|| format!("{name} failed")),
                            kind: text("exception.type"),
                            frames: vec![Frame { function: Some(name.to_owned()), own: true, ..Default::default() }],
                            trace: text("exception.stacktrace"),
                            release: release.clone(),
                            extra: json!({ "attributes": attrs, "span": name, "trace_id": span["traceId"] }),
                            ..Default::default()
                        },
                    };
                    super::record(hub, incoming).await?;
                    kept += 1;
                }
            }
        }
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_attributes_and_levels() {
        let a = attributes(&json!([
            { "key": "exception.type", "value": { "stringValue": "KeyError" } },
            { "key": "code.lineno", "value": { "intValue": "42" } }
        ]));
        assert_eq!(a["exception.type"], "KeyError");
        assert_eq!(a["code.lineno"], 42);
        assert_eq!(level(17, None), Level::Error);
        assert_eq!(level(13, None), Level::Warning);
        assert_eq!(level(9, None), Level::Info);
        assert_eq!(level(0, Some("WARN")), Level::Warning);
        assert_eq!(nanos_ms(&json!("1761925262500000000")), Some(1_761_925_262_500));
    }
}
