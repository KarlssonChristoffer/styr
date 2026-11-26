//! What a Sentry SDK sends to `/api/<n>/envelope/` or `/api/<n>/store/`: checking the source's
//! key, decoding the body and splitting the envelope, then events into problems (`from_sentry`),
//! check-ins to `quiet.rs` and transactions to `metrics.rs`. Keeping a problem is `record` in `mod.rs`.

use super::*;

/// The key a Sentry SDK shows: the `X-Sentry-Auth` header, `?sentry_key=`, or the DSN in the
/// envelope's own header.
pub fn shown_key(headers: &axum::http::HeaderMap, query: Option<&str>, body: &[u8]) -> Option<String> {
    let from_header = headers.get("x-sentry-auth").and_then(|v| v.to_str().ok()).and_then(|v| {
        v.trim_start_matches("Sentry").split(',').find_map(|p| p.trim().strip_prefix("sentry_key=").map(str::to_owned))
    });
    let from_query = || query?.split('&').find_map(|p| p.strip_prefix("sentry_key=").map(str::to_owned));
    let from_body = || {
        let first = body.split(|b| *b == b'\n').next()?;
        let header: Value = serde_json::from_slice(first).ok()?;
        let dsn = header["dsn"].as_str()?;
        let rest = dsn.split("://").nth(1)?;
        Some(rest.split('@').next()?.split(':').next()?.to_owned())
    };
    from_header.or_else(from_query).or_else(from_body)
}

/// Takes what a Sentry SDK sent to `/api/<id>/envelope/` or `/api/<id>/store/`. Returns how many
/// items it kept: events become problems, check-ins feed the quiet-source watch, and
/// transactions become timings (`metrics`). Sessions, replays and the rest are skipped.
pub async fn ingest(hub: &Arc<Hub>, id: u32, key: Option<&str>, encoding: Option<&str>, body: &[u8], envelope: bool) -> Result<usize, IngestError> {
    let source = source_for(hub, id, key).await?;
    let body = decode(encoding, body).map_err(|e| IngestError::Bad(format!("{e:#}")))?;
    let items = if envelope { envelope_events(&body).map_err(|e| IngestError::Bad(format!("{e:#}")))? } else { vec![("event".to_owned(), parse_json(&body)?)] };
    let bad = |e: anyhow::Error| IngestError::Bad(format!("{e:#}"));
    let mut kept = 0;
    for (kind, item) in items {
        match kind.as_str() {
            // Debug messages are for the program's own log, not for a list of problems.
            "event" if item["level"] == "debug" => continue,
            "event" => {
                record(hub, from_sentry(&source, &item)).await.map_err(bad)?;
            }
            "check_in" => quiet::check_in(hub, &source, &item).await.map_err(bad)?,
            "transaction" => metrics::transaction(hub, &source, &item).await.map_err(bad)?,
            _ => continue,
        }
        kept += 1;
    }
    Ok(kept)
}

/// The source with this id, if the key is its own.
pub(crate) async fn source_for(hub: &Hub, id: u32, key: Option<&str>) -> Result<Source, IngestError> {
    let source = sources(hub).await.into_iter().find(|s| s.id == id).ok_or(IngestError::Unknown)?;
    if key != Some(source.key.as_str()) {
        return Err(IngestError::Key);
    }
    quiet::heard(&source.name);
    Ok(source)
}

#[derive(Debug)]
pub enum IngestError {
    Unknown,
    Key,
    Bad(String),
}

pub(super) fn parse_json(body: &[u8]) -> Result<Value, IngestError> {
    serde_json::from_slice(body).map_err(|e| IngestError::Bad(format!("not JSON: {e}")))
}

pub(super) fn decode(encoding: Option<&str>, body: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    match encoding.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        None | Some("") | Some("identity") => return Ok(body.to_vec()),
        Some("gzip") => flate2::read::GzDecoder::new(body).take(64 << 20).read_to_end(&mut out),
        Some("deflate") => flate2::read::ZlibDecoder::new(body).take(64 << 20).read_to_end(&mut out),
        Some("br") => brotli_decompressor::Decompressor::new(body, 4096).take(64 << 20).read_to_end(&mut out),
        Some(other) => bail!("unknown Content-Encoding {other}"),
    }
    .context("unpacking the body")?;
    Ok(out)
}

/// The items in a Sentry envelope, by type: a header line, then items, each a header line with
/// an optional `length` and its payload.
pub(super) fn envelope_events(body: &[u8]) -> Result<Vec<(String, Value)>> {
    let mut rest = body;
    let line = |rest: &mut &[u8]| -> Option<Vec<u8>> {
        if rest.is_empty() {
            return None;
        }
        let end = rest.iter().position(|b| *b == b'\n').unwrap_or(rest.len());
        let out = rest[..end].to_vec();
        *rest = &rest[(end + 1).min(rest.len())..];
        Some(out)
    };
    line(&mut rest).context("an empty envelope")?;
    let mut events = Vec::new();
    while let Some(header) = line(&mut rest) {
        if header.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let header: Value = serde_json::from_slice(&header).context("an item header that is not JSON")?;
        let payload = match header["length"].as_u64() {
            Some(n) => {
                let n = (n as usize).min(rest.len());
                let out = rest[..n].to_vec();
                rest = &rest[n..];
                if rest.first() == Some(&b'\n') {
                    rest = &rest[1..];
                }
                out
            }
            None => line(&mut rest).unwrap_or_default(),
        };
        if let Some(kind) = header["type"].as_str()
            && let Ok(item) = serde_json::from_slice::<Value>(&payload)
        {
            events.push((kind.to_owned(), item));
        }
    }
    Ok(events)
}

/// A Sentry event in Hub's terms.
pub(super) fn from_sentry(source: &Source, event: &Value) -> Incoming {
    let values = |v: &Value| -> Vec<Value> {
        match v {
            Value::Array(a) => a.clone(),
            Value::Object(o) => o.get("values").and_then(Value::as_array).cloned().unwrap_or_default(),
            _ => Vec::new(),
        }
    };
    let text = |v: &Value| -> Option<String> {
        match v {
            Value::String(s) => Some(s.clone()),
            Value::Object(o) => o.get("formatted").or_else(|| o.get("message")).and_then(Value::as_str).map(str::to_owned),
            _ => None,
        }
    };
    let exceptions = values(&event["exception"]);
    // The last exception is the one raised last, the one the program saw.
    let exception = exceptions.last();
    let frames = exception
        .map(|e| values(&e["stacktrace"]["frames"]))
        .or_else(|| Some(values(&event["stacktrace"]["frames"])))
        .unwrap_or_default()
        .iter()
        .map(|f| Frame {
            file: f["filename"].as_str().or_else(|| f["abs_path"].as_str()).or_else(|| f["module"].as_str()).map(str::to_owned),
            function: f["function"].as_str().map(str::to_owned),
            line: f["lineno"].as_u64().map(|l| l as u32),
            code: f["context_line"].as_str().map(|s| s.trim().to_owned()),
            own: f["in_app"].as_bool().unwrap_or(false),
        })
        .collect();
    let message = exception
        .and_then(|e| e["value"].as_str().map(str::to_owned))
        .or_else(|| text(&event["logentry"]))
        .or_else(|| text(&event["message"]))
        .unwrap_or_default();
    let crumbs = values(&event["breadcrumbs"])
        .iter()
        .map(|b| Crumb {
            ts: sentry_time(&b["timestamp"]),
            level: b["level"].as_str().map(str::to_owned),
            category: b["category"].as_str().or_else(|| b["type"].as_str()).map(str::to_owned),
            message: b["message"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| if b["data"].is_null() { String::new() } else { b["data"].to_string() }),
        })
        .collect();
    let mut extra = serde_json::Map::new();
    for key in ["tags", "extra", "contexts", "user", "server_name", "platform", "logger", "transaction", "sdk"] {
        if !event[key].is_null() {
            extra.insert(key.to_owned(), event[key].clone());
        }
    }
    let fingerprint = event["fingerprint"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect());
    Incoming {
        project: source.name.clone(),
        source: "sentry".into(),
        level: event["level"].as_str().map(Level::parse).unwrap_or(if exception.is_some() { Level::Error } else { Level::Info }),
        folder: source.folder.clone(),
        fingerprint,
        hit: Hit {
            ts: sentry_time(&event["timestamp"]).unwrap_or_else(now_ms),
            message,
            kind: exception.and_then(|e| e["type"].as_str().map(str::to_owned)),
            frames,
            trace: None,
            crumbs,
            release: event["release"].as_str().map(str::to_owned),
            environment: event["environment"].as_str().map(str::to_owned),
            extra: Value::Object(extra),
        },
    }
}

/// Sentry's times: seconds since 1970 as a number, or an RFC 3339 string.
pub(super) fn sentry_time(v: &Value) -> Option<i64> {
    if let Some(secs) = v.as_f64() {
        return Some((secs * 1000.0) as i64);
    }
    let s = v.as_str()?;
    // 2026-10-31T15:41:02.123Z, without pulling in a date crate.
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let time = time.trim_end_matches('Z');
    let time = time.split(['+']).next()?;
    let mut t = time.split(':');
    let (h, min) = (t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?);
    let sec: f64 = t.next().unwrap_or("0").parse().ok()?;
    // Days from 1970-01-01 (Howard Hinnant's days_from_civil).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * DAY_MS + h * 3_600_000 + min * 60_000 + (sec * 1000.0) as i64)
}
