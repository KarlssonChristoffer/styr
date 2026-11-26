//! Handlers of the tools that reach other parts of Hub: Customs (search, declare and its vetting
//! note), recipes (list, suggest, run), the Hub status, plugin search, a project's API connections,
//! `ask_jev` and own-tool actions. Each is a thin call into its module (`customs.rs`, `recipes.rs`,
//! `plugins.rs`, `own_tools.rs`); their declarations are `registry.rs`.

use crate::locked::ResultExt;
use super::*;

pub(super) async fn api_connections(hub: &Arc<Hub>, caller: &Caller) -> Result<Value> {
    match caller {
        Caller::Thread(t) => hub.api_brief(&t.cwd).await,
        Caller::Agent { .. } => bail!("api_connections is for a thread in a project"),
    }
}

pub(super) async fn ask_jev(hub: &Arc<Hub>, caller: &Caller, args: &Value) -> Result<Value> {
    let Caller::Thread(t) = caller else { bail!("ask_jev is for a thread in Hub") };
    let question = args["question"].as_str().map(str::trim).filter(|q| !q.is_empty()).context("question is missing")?;
    let text = args["text"].as_str().unwrap_or_default();
    let options: Vec<(String, String)> = match args["options"].as_object() {
        Some(o) if (2..=8).contains(&o.len()) => o.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or(k).to_owned())).collect(),
        Some(o) if !o.is_empty() => bail!("options takes 2 to 8 entries"),
        _ => Vec::new(),
    };
    let items: Vec<String> = args["items"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()).unwrap_or_default();
    Ok(hub.ask_for_agent(&t.id, question, text, options, items).await)
}

pub(super) async fn list_recipes(hub: &Arc<Hub>) -> Result<Value> {
    let all = crate::recipes::list(&hub.ctx()).await?.recipes;
    let item = |r: &crate::model::Recipe| {
        let mut v = json!({ "name": r.name, "about": r.about });
        if let Some(folder) = &r.folder {
            v["only_in"] = json!(folder);
        }
        if let Some(run) = r.runs.first() {
            v["last_run"] = json!({ "thread": run.thread, "link": link("thread", &run.thread), "folder": run.folder });
        }
        v
    };
    let kept: Vec<Value> = all.iter().filter(|r| r.suggested.is_none()).map(item).collect();
    let waiting: Vec<Value> = all.iter().filter(|r| r.suggested.is_some()).map(|r| json!({ "name": r.name, "about": r.about })).collect();
    Ok(json!({ "recipes": kept, "suggested_waiting_for_user": waiting }))
}

pub(super) async fn suggest_recipe(hub: &Arc<Hub>, caller: &Caller, args: &Value) -> Result<Value> {
    let text = |k: &str| args[k].as_str().map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
    let draft = crate::model::RecipeDraft {
        name: text("name").context("name is missing")?,
        about: text("about"),
        prompt: text("prompt").context("prompt is missing")?,
        engine: None,
        folder: None,
        template: None,
    };
    let by = match caller {
        Caller::Thread(t) => crate::model::RecipeSuggestion { by: t.title.clone(), thread: Some(t.id.clone()), why: text("why").unwrap_or_default() },
        Caller::Agent { name, .. } => crate::model::RecipeSuggestion { by: name.clone(), thread: None, why: text("why").unwrap_or_default() },
    };
    let recipe = crate::recipes::suggest(&hub.ctx(), draft, by).await?;
    Ok(json!({
        "suggested": recipe.name,
        "note": "It waits on the Recipes page (Control > Recipes) until the user keeps it. Tell the user in a line.",
    }))
}

pub(super) async fn run_recipe(hub: &Arc<Hub>, caller: &Caller, args: &Value) -> Result<Value> {
    let recipe = crate::recipes::find(&hub.ctx(), args["recipe"].as_str().context("recipe is missing")?).await?;
    let own = match caller {
        Caller::Thread(t) => Some(t),
        Caller::Agent { .. } => None,
    };
    let folder = match (args["folder"].as_str(), args["project"].as_str()) {
        (Some(f), _) if !f.trim().is_empty() => Some(f.trim().to_owned()),
        (_, Some(p)) => Some(project_folder(hub, p).await?),
        _ => own.map(|t| t.cwd.clone()),
    };
    if let (Some(own), Some(folder)) = (own, folder.as_deref())
        && !same_folder(folder, &own.cwd)
        && crate::desk::is_desk(&hub.ctx(), own).await
    {
        bail!("you are the desk of {}; run recipes in this project only", own.cwd);
    }
    // As with start_thread, a run is never allowed more than the thread that started it.
    let engine = recipe.engine.clone().unwrap_or_else(|| hub.config.default_engine.clone());
    let settings: BTreeMap<String, Value> = match own {
        Some(t) if t.engine == engine => hub.store.settings(&t.id).await?.into_iter().collect(),
        _ => BTreeMap::new(),
    };
    let how = crate::recipes::Start { note: args["note"].as_str().map(str::to_owned), settings, by: Some(caller.mark()) };
    let thread = hub.start_recipe(&recipe.id, folder.as_deref(), how).await?;
    let thread = hub.store.thread(&thread).await?.context("the run's thread is gone")?;
    Ok(json!({ "thread": brief(&thread) }))
}

pub(super) async fn run_tool_action(hub: &Arc<Hub>, caller: &Caller, args: &Value) -> Result<Value> {
    let ask: crate::model::RunOwnToolAction = serde_json::from_value(args.clone()).context("folder and action are needed")?;
    let ran = crate::own_tools::run(&hub.ctx(), &ask, caller.access() == AgentAccess::Full).await?;
    Ok(serde_json::to_value(ran)?)
}

pub(super) async fn status(hub: &Arc<Hub>) -> Result<Value> {
    let threads: Vec<Thread> = hub.all_threads().await?.into_iter().filter(|t| !t.archived).collect();
    let pick = |s: ThreadStatus| threads.iter().filter(|t| t.status == s).map(brief).collect::<Vec<_>>();
    let mut engines = Vec::new();
    for name in hub.config.engines.keys().filter(|n| *n != crate::sim::ENGINE) {
        let models = models(hub, name).await.map(|(_, m)| m.into_iter().map(|(v, n)| json!({ "value": v, "name": n })).collect());
        engines.push(json!({
            "name": name,
            "default": *name == hub.config.default_engine,
            "models": models.unwrap_or_else(Vec::<Value>::new),
        }));
    }
    let projects: Vec<Value> =
        hub.store.projects().await?.into_iter().map(|p| json!({ "name": p.name, "link": link("project", &p.path), "folder": p.path })).collect();
    Ok(json!({
        "needs_you": pick(ThreadStatus::WaitingOnYou),
        "working": pick(ThreadStatus::Running),
        "failed": pick(ThreadStatus::Error),
        "recent": threads.iter().take(10).map(brief).collect::<Vec<_>>(),
        "threads": threads.len(),
        "projects": projects,
        "engines": engines,
    }))
}

pub(super) async fn plugins_search(hub: &Arc<Hub>, args: &Value) -> Result<Value> {
    let query = args["query"].as_str().unwrap_or_default().trim();
    let limit = args["limit"].as_u64().unwrap_or(10).clamp(1, 30) as usize;
    let (repos, message) = if query.is_empty() {
        let d = crate::plugins::discover_counted(&hub.ctx()).await;
        (d.rising, d.message)
    } else {
        let s = crate::plugins::search(&hub.ctx(), query).await;
        (s.repos, s.message)
    };
    if let Some(m) = message
        && repos.is_empty()
    {
        anyhow::bail!(m);
    }
    let items: Vec<Value> = repos
        .iter()
        .filter(|r| args["kind"].as_str().is_none_or(|k| r.kind == k))
        .take(limit)
        .map(|r| {
            json!({
                "repo": r.full_name, "kind": r.kind, "description": r.description, "stars": r.stars,
                "stars_this_week": r.week, "language": r.language, "license": r.license,
                "customs": r.customs.map(|c| format!("{c:?}").to_lowercase()).unwrap_or_else(|| "not checked".into()),
                "url": r.url,
            })
        })
        .collect();
    Ok(json!({ "items": items, "note": "Show the best one or two to the user with suggest_plugin and why they fit." }))
}

pub(super) async fn customs_search(hub: &Arc<Hub>, args: &Value) -> Result<Value> {
    let list = crate::customs::list(&hub.ctx()).await?;
    let limit = args["limit"].as_u64().unwrap_or(50).clamp(1, 500) as usize;
    let hits = crate::customs::search(&list, args["query"].as_str().unwrap_or_default(), args["kind"].as_str(), args["category"].as_str());
    let total = hits.len();
    let items: Vec<Value> = hits
        .into_iter()
        .take(limit)
        .map(|m| {
            let mut v = json!({ "kind": m.kind, "name": m.name, "status": m.status, "category": m.category, "summary": m.summary });
            if !m.versions.is_empty() {
                v["versions"] = json!(m.versions);
            }
            if m.status != crate::model::CustomsStatus::Cleared {
                v["reason"] = json!(m.reason);
            }
            v
        })
        .collect();
    let mut out = json!({ "items": items, "total": total });
    if total == 0 {
        out["note"] = json!("Nothing matches. Use something cleared, or declare it with customs_declare.");
    }
    Ok(out)
}

pub(super) async fn customs_declare(hub: &Arc<Hub>, caller: &Caller, args: &Value) -> Result<Value> {
    let kind = args["kind"].as_str().context("kind is missing")?;
    let name = args["name"].as_str().context("name is missing")?;
    let why = args["why"].as_str().unwrap_or_default();
    let summary = args["summary"].as_str().unwrap_or_default();
    let thread_id = match caller {
        Caller::Thread(t) => Some(t.id.as_str()),
        _ => None,
    };
    let m = crate::customs::request(&hub.ctx(), kind, name, why, summary, "agent", thread_id).await?;
    // Through Customs now, so a stamp lets the agent carry on in the same turn. When the user chose
    // who vets in this kind of thread (Settings > Handoffs), the thread shows a note of it.
    let cwd = match caller {
        Caller::Thread(t) => Some(t.cwd.as_str()),
        _ => None,
    };
    let chosen = match thread_id {
        Some(_) => crate::handoff::pick(&hub.ctx(), "vetting", cwd).await,
        None => None,
    };
    let started = now_ms();
    let note_id = format!("vetting-{started}");
    let note = |p: &crate::model::HandoffPick| crate::model::TaskData {
        id: note_id.clone(),
        task: "vetting".into(),
        name: m.name.clone(),
        kind: m.kind.clone(),
        engine: p.engine.clone(),
        model: Some(p.model.clone()),
        state: "working".into(),
        by: "hub".into(),
        ..Default::default()
    };
    if let (Some(thread_id), Some(p)) = (thread_id, &chosen) {
        hub.emit_typed(thread_id, &note(p)).await.warn_on_err("emit Task");
    }
    let settled = crate::customs_review::settle_for(hub, &m.kind, &m.name, cwd).await;
    // The checks alone can settle it, and then no model ran: the note must not name one.
    let ran = crate::customs_review::model_ran(&m.kind, &m.name);
    if let (Some(thread_id), Some(p)) = (thread_id, &chosen) {
        let mut data = crate::model::TaskData { ms: Some(now_ms() - started), ..note(p) };
        if !ran {
            data.engine = String::new();
            data.model = Some(None);
        }
        match &settled {
            Ok(done) => {
                let done = done.as_ref().unwrap_or(&m);
                data.state = "done".into();
                data.verdict = Some(
                    match done.status {
                        crate::model::CustomsStatus::Cleared => "cleared",
                        crate::model::CustomsStatus::Held => "held",
                        crate::model::CustomsStatus::Refused => "refused",
                    }
                    .into(),
                );
                data.reason = Some(done.reason.clone());
            }
            Err(e) => {
                data.state = "failed".into();
                data.reason = Some(format!("{e:#}"));
            }
        }
        hub.emit_typed(thread_id, &data).await.warn_on_err("emit Task");
    }
    let m = settled?.unwrap_or(m);
    // Still held after the review: the thread waits on the user for this, so the thread is where they can clear it.
    if let Caller::Thread(thread) = caller
        && m.status == crate::model::CustomsStatus::Held
    {
        let found = crate::model::CustomsFound { kind: m.kind.clone(), name: m.name.clone(), path: String::new(), status: "held".into(), why: Some(why.to_owned()) };
        hub.emit_typed(&thread.id, &crate::model::CustomsData { declared: Some(true), found: vec![found], ..Default::default() }).await?;
    }
    let note = match m.status {
        crate::model::CustomsStatus::Cleared => {
            "Cleared: it has the customs stamp, so you and every other agent on every machine may use it. Go ahead."
        }
        crate::model::CustomsStatus::Held => {
            "Held: Customs could not verify it, so it waits for the user to clear it. Do not install or use it \
             yet: carry on with the rest of the work, or use something cleared if one fits, and tell the user \
             it is waiting at Customs."
        }
        crate::model::CustomsStatus::Refused => "Refused: do not use it. The reason says why; find something else.",
    };
    Ok(json!({ "kind": m.kind, "name": m.name, "status": m.status, "reason": m.reason, "note": note }))
}
