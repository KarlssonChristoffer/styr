//! Reading a tool: its `hub.toml`, then each distinct status or list command once, with the
//! headline, state, detail and list items picked out of the JSON it printed (`render` fills a
//! pointer or a `{/pointer}` template). Readings are kept a few seconds unless asked fresh; a
//! failure stays on the part it broke. Parsing the file and running actions are `own_tools.rs`.

use super::*;

/// Every tool found, read now or a few seconds ago.
pub async fn read_all(folders: Vec<PathBuf>, fresh: bool) -> Vec<OwnTool> {
    let reads = folders.into_iter().map(|folder| async move {
        if !fresh
            && let Some((at, tool)) = READINGS.lock().unwrap_or_else(|e| e.into_inner()).get(&folder)
            && at.elapsed() < FRESH
        {
            return tool.clone();
        }
        let tool = read(&folder).await;
        READINGS.lock().unwrap_or_else(|e| e.into_inner()).insert(folder, (Instant::now(), tool.clone()));
        tool
    });
    let mut tools = futures::future::join_all(reads).await;
    tools.sort_by_key(|t| t.name.to_lowercase());
    tools
}

/// One tool: its file, then its commands, each failure kept where it happened.
pub async fn read(folder: &Path) -> OwnTool {
    let mut tool = OwnTool {
        name: folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        about: None,
        folder: folder.display().to_string(),
        headline: None,
        state: None,
        detail: None,
        lists: Vec::new(),
        actions: Vec::new(),
        needs_you: 0,
        handoff: None,
        worker: None,
        error: None,
        checked_at: now_ms(),
    };
    let manifest = match std::fs::read_to_string(folder.join(FILE)).map_err(anyhow::Error::from).and_then(|t| parse(&t)) {
        Ok(m) => m,
        Err(e) => {
            tool.error = Some(format!("{FILE}: {e:#}"));
            return tool;
        }
    };
    tool.name = manifest.name.clone();
    tool.about = manifest.about.clone();
    tool.handoff = manifest.handoff.as_ref().map(|h| OwnToolHandoff { label: h.label.clone(), about: h.about.clone(), can_build: h.build.is_some() });
    tool.worker = manifest.worker.as_ref().map(|w| w.label.clone());
    tool.actions = manifest
        .actions
        .iter()
        .map(|a| OwnToolAction { id: a.id.clone(), label: a.label.clone(), on: a.on.clone(), ask: a.ask.clone(), danger: a.danger, about: a.about.clone() })
        .collect();

    // Each distinct command runs once per reading, however many parts read its document.
    let mut docs: HashMap<Vec<String>, Result<Value, String>> = HashMap::new();
    let mut wanted: Vec<&Vec<String>> = manifest.status.iter().map(|s| &s.run).collect();
    wanted.extend(manifest.lists.iter().filter_map(|l| l.run.as_ref()));
    for run in wanted {
        if !docs.contains_key(run) {
            let doc = document(folder, &manifest.env, run).await.map_err(|e| format!("{e:#}"));
            docs.insert(run.clone(), doc);
        }
    }

    if let Some(status) = &manifest.status {
        match &docs[&status.run] {
            Ok(doc) => {
                tool.headline = status.headline.as_deref().and_then(|t| render(t, doc));
                tool.state = status.state.as_deref().and_then(|t| render(t, doc));
                tool.detail = status.detail.as_deref().and_then(|t| render(t, doc));
            }
            Err(e) => tool.error = Some(e.clone()),
        }
    }
    for list in &manifest.lists {
        let run = list.run.as_ref().or(manifest.status.as_ref().map(|s| &s.run)).expect("checked by parse");
        let mut out = OwnToolList { title: list.title.clone(), needs_you: list.needs_you, items: Vec::new(), total: 0, error: None };
        match &docs[run] {
            Ok(doc) => match items(list, doc) {
                Ok((items, total)) => {
                    out.items = items;
                    out.total = total;
                }
                Err(e) => out.error = Some(e),
            },
            Err(e) => out.error = Some(e.clone()),
        }
        if list.needs_you {
            tool.needs_you += out.total;
        }
        tool.lists.push(out);
    }
    tool
}

pub(super) fn items(list: &List, doc: &Value) -> Result<(Vec<OwnToolItem>, u32), String> {
    let array = match doc.pointer(&list.items) {
        Some(Value::Array(a)) => a,
        Some(Value::Null) | None => return Ok((Vec::new(), 0)),
        Some(_) => return Err(format!("{} is not a list", list.items)),
    };
    let keep = |item: &&Value| {
        list.filter.iter().all(|(key, want)| {
            let pointer = if key.starts_with('/') { key.clone() } else { format!("/{key}") };
            let have = item.pointer(&pointer).map(plain).unwrap_or_default();
            match want {
                toml::Value::Array(any) => any.iter().any(|w| toml_plain(w) == have),
                one => toml_plain(one) == have,
            }
        })
    };
    let mut picked: Vec<&Value> = array.iter().filter(keep).collect();
    if list.reverse {
        picked.reverse();
    }
    let total = picked.len() as u32;
    let items = picked
        .into_iter()
        .take(list.limit.unwrap_or(LIMIT))
        .map(|item| OwnToolItem {
            id: list.id.as_deref().and_then(|t| render(t, item)),
            text: render(&list.text, item).unwrap_or_default(),
            state: list.state.as_deref().and_then(|t| render(t, item)),
            detail: list.detail.as_deref().and_then(|t| render(t, item)),
            link: list.link.as_deref().and_then(|t| render(t, item)),
        })
        .collect();
    Ok((items, total))
}

/// A field of the document: a JSON pointer (`/headline`), or text with pointers in braces
/// (`Capture is {/overall}`). Nothing when what it names is missing or empty.
pub fn render(template: &str, doc: &Value) -> Option<String> {
    let text = if template.starts_with('/') && !template.contains('{') {
        doc.pointer(template).map(plain).unwrap_or_default()
    } else {
        let mut out = String::new();
        let mut rest = template;
        while let Some(start) = rest.find("{/") {
            out.push_str(&rest[..start]);
            let Some(len) = rest[start..].find('}') else {
                out.push_str(&rest[start..]);
                rest = "";
                break;
            };
            out.push_str(&doc.pointer(&rest[start + 1..start + len]).map(plain).unwrap_or_default());
            rest = &rest[start + len + 1..];
        }
        out.push_str(rest);
        // A template whose fields were all missing leaves separators behind.
        out.split_whitespace().collect::<Vec<_>>().join(" ").trim_matches(|c: char| c == '·' || c == '-' || c.is_whitespace()).to_owned()
    };
    let text = text.trim().to_owned();
    (!text.is_empty()).then_some(text)
}
