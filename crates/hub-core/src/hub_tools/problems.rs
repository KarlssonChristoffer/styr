//! The handlers of the problem tools (Vakten's): list, read and dossier a problem, change or stamp
//! it, start its fix thread with the contract (repro, done) and tell the user. A `machine` other
//! than this one sends the call to that machine's `/api/problems/...`. Also `your_tools` and
//! `review_tool`, which read `own_tools.rs` and save to `problems/tools/`.

use super::*;

pub(super) async fn dossier_tool(hub: &Arc<Hub>, args: &Value) -> Result<Value> {
    match (args["id"].as_i64(), other_machine(hub, args)) {
        (Some(id), Some(host)) => hub.remotes.get_from::<Value>(&host, &format!("/api/problems/{id}/dossier")).await,
        (Some(id), None) => crate::problems::dossier::dossier(hub, id).await.and_then(|d| Ok(serde_json::to_value(d)?)),
        (None, _) => Err(anyhow::anyhow!("id is missing")),
    }
}

pub(super) async fn tell_tool(hub: &Arc<Hub>, args: &Value) -> Result<Value> {
    let text = args["text"].as_str().map(str::trim).filter(|t| !t.is_empty()).context("text is missing")?;
    let critical = args["critical"].as_bool().unwrap_or(false);
    match (other_machine(hub, args), args["id"].as_i64()) {
        // The problem's activity is kept where it is; the notification goes out here.
        (Some(host), Some(id)) => {
            let told = hub.remotes.post_to::<Value>(&host, &format!("/api/problems/guard/{id}/tell"), &json!({ "text": text }), REMOTE_WAIT).await;
            let said = crate::problems::vakten::tell(hub, None, text, critical).await;
            told.and(said).map(|_| json!({ "told": true }))
        }
        _ => crate::problems::vakten::tell(hub, args["id"].as_i64(), text, critical).await.map(|_| json!({ "told": true })),
    }
}

pub(super) async fn review_tool(hub: &Arc<Hub>, args: &Value) -> Result<Value> {
    let findings = serde_json::from_value::<Vec<crate::problems::tools::ToolFinding>>(args["findings"].clone()).map_err(|e| anyhow::anyhow!("findings: {e}"))?;
    crate::problems::tools::save_review(hub, args["text"].as_str().unwrap_or_default(), findings).await.map(|r| json!({ "saved": r.ts }))
}

pub(super) async fn your_tools(hub: &Arc<Hub>, args: &Value) -> Result<Value> {
    serde_json::to_value(crate::own_tools::list(&hub.ctx(), args["fresh"].as_bool().unwrap_or(false)).await).map_err(Into::into)
}

/// How long a problem tool waits for another machine.
pub(super) const REMOTE_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// The other machine a problem tool's `machine` names, None for this one.
pub(super) fn other_machine(hub: &Hub, args: &Value) -> Option<String> {
    let name = args["machine"].as_str().map(str::trim).filter(|m| !m.is_empty())?;
    (!name.eq_ignore_ascii_case(hub.remotes.local_name())).then(|| name.to_owned())
}

pub(super) async fn problems_tool(hub: &Arc<Hub>, args: &Value) -> Result<Value> {
    let filter = crate::problems::ProblemFilter {
        status: args.get("status").cloned().and_then(|v| serde_json::from_value(v).ok()),
        view: args.get("view").cloned().and_then(|v| serde_json::from_value(v).ok()),
        level: Some(args.get("level").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or(crate::problems::Level::Warning)),
        project: args["project"].as_str().map(str::to_owned),
        source: args["source"].as_str().map(str::to_owned),
        query: args["query"].as_str().map(str::to_owned),
        limit: Some(args["limit"].as_u64().unwrap_or(50).min(500) as u32),
    };
    // This machine and every other one online, or the one asked for. Each problem says which
    // machine to name when acting on it.
    let me = hub.remotes.local_name().to_owned();
    let mut problems: Vec<(String, crate::problems::Problem)> = Vec::new();
    let only = args["machine"].as_str().map(str::trim).filter(|m| !m.is_empty());
    if only.is_none_or(|m| m.eq_ignore_ascii_case(&me)) {
        problems.extend(crate::problems::list(hub, filter.clone()).await?.into_iter().map(|p| (me.clone(), p)));
    }
    let hosts: Vec<String> = match (only, other_machine(hub, args)) {
        (Some(_), Some(host)) => vec![host],
        (Some(_), None) => Vec::new(),
        (None, _) => hub.remotes.online(),
    };
    for host in hosts {
        let mut url = reqwest::Url::parse("http://hub/api/problems")?;
        {
            let mut q = url.query_pairs_mut();
            for (k, v) in serde_json::to_value(&filter)?.as_object().into_iter().flatten() {
                match v {
                    Value::String(t) => q.append_pair(k, t),
                    Value::Number(n) => q.append_pair(k, &n.to_string()),
                    _ => &mut q,
                };
            }
        }
        let path = format!("{}?{}", url.path(), url.query().unwrap_or_default());
        match hub.remotes.get_from::<Vec<crate::problems::Problem>>(&host, &path).await {
            Ok(ps) => problems.extend(ps.into_iter().map(|p| (host.clone(), p))),
            Err(e) if only.is_some() => return Err(e),
            Err(_) => {}
        }
    }
    problems.sort_by_key(|(_, p)| std::cmp::Reverse(p.last_seen));
    Ok(json!({ "problems": problems.iter().map(|(machine, p)| json!({
        "id": p.id, "title": p.title, "project": p.project, "source": p.source, "level": p.level, "where": p.culprit,
        "machine": machine, "count": p.count, "first_seen": p.first_seen, "last_seen": p.last_seen,
        "status": p.status, "came_back": p.regressed, "escalating": p.escalating, "new": p.new, "needs_user": p.needs_you,
        "prio": p.prio, "lane": p.lane, "category": p.category, "stamped_by": p.stamped_by,
        "thread": p.thread.as_ref().map(|t| link("thread", t)), "pr": p.pr.as_ref().map(|pr| &pr.url),
    })).collect::<Vec<_>>() }))
}

pub(super) async fn problem_tool(hub: &Arc<Hub>, args: &Value) -> Result<Value> {
    let id = args["id"].as_i64().context("id is missing")?;
    let hits = args["hits"].as_u64().unwrap_or(3).min(30) as u32;
    if let Some(host) = other_machine(hub, args) {
        return hub.remotes.get_from::<Value>(&host, &format!("/api/problems/{id}")).await;
    }
    let detail = crate::problems::detail(hub, id, hits).await?.with_context(|| format!("no problem {id}"))?;
    Ok(serde_json::to_value(detail)?)
}

/// Who a change on a problem is from: Vakten's own thread, another thread by its title, or an agent.
pub(super) async fn problem_actor(hub: &Arc<Hub>, caller: &Caller) -> String {
    match caller {
        Caller::Thread(t) if crate::problems::vakten::is_vakten(hub, &t.id).await => "Vakten".into(),
        Caller::Thread(t) if !t.title.trim().is_empty() => t.title.clone(),
        Caller::Thread(_) => "a thread".into(),
        Caller::Agent { name, .. } => name.clone(),
    }
}

pub(super) async fn change_problem(hub: &Arc<Hub>, caller: &Caller, args: &Value, stamp: bool) -> Result<Value> {
    let id = args["id"].as_i64().context("id is missing")?;
    let mut patch: crate::problems::ProblemPatch = serde_json::from_value(args.clone()).context("reading the change")?;
    // Each tool does its own half, so a stamp never resolves and a change never stamps.
    if stamp {
        patch = crate::problems::ProblemPatch {
            prio: patch.prio,
            lane: patch.lane,
            category: patch.category,
            confidence: patch.confidence,
            fixable: patch.fixable,
            note: patch.note,
            why: patch.why,
            ..Default::default()
        };
    } else {
        patch = crate::problems::ProblemPatch {
            status: patch.status,
            resolve: patch.resolve,
            archive: patch.archive,
            merge_into: patch.merge_into,
            unmerge: patch.unmerge,
            why: patch.why,
            ..Default::default()
        };
    }
    let by = problem_actor(hub, caller).await;
    let p = match other_machine(hub, args) {
        Some(host) => {
            let path = format!("/api/problems/guard/{id}/change");
            hub.remotes.post_to::<crate::problems::Problem>(&host, &path, &json!({ "by": by, "patch": patch }), REMOTE_WAIT).await?
        }
        None => crate::problems::change(hub, id, patch, &by).await?,
    };
    Ok(json!({ "id": p.id, "status": p.status, "prio": p.prio, "lane": p.lane, "category": p.category, "merged_into": p.merged_into }))
}

pub(super) async fn problem_thread(hub: &Arc<Hub>, caller: &Caller, args: &Value) -> Result<Value> {
    let id = args["id"].as_i64().context("id is missing")?;
    let by = problem_actor(hub, caller).await;
    let host = other_machine(hub, args);
    let arg = |k: &str| args[k].as_str().map(str::trim).unwrap_or_default().to_owned();
    let again = args["again"].as_bool().unwrap_or(false);
    let detail: crate::problems::ProblemDetail = match &host {
        Some(h) => hub.remotes.get_from(h, &format!("/api/problems/{id}")).await?,
        None => crate::problems::detail(hub, id, 1).await?.with_context(|| format!("no problem {id}"))?,
    };
    let p = detail.problem.clone();
    // The contract: what is given here, else what the note already holds (stamp_problem's note
    // takes repro and done too).
    let mut note = p.note.clone().unwrap_or_default();
    let before = note.clone();
    for (field, value) in [(&mut note.repro, arg("repro")), (&mut note.cause, arg("cause")), (&mut note.done, arg("done"))] {
        if !value.is_empty() {
            *field = value;
        }
    }
    if note.plan.trim().is_empty() {
        note.plan = arg("plan");
    }
    if by == "Vakten" {
        if note.repro.trim().is_empty() || note.done.trim().is_empty() {
            bail!("write the contract first: repro (how to make it happen) and done (what must be true when it is fixed), here or in the note");
        }
        match &host {
            Some(_) => {
                crate::problems::vakten::may_fix_problem(&p, again)?;
                // Its thread may be here (`fix_here`); the machine the problem is on checks its own.
                if again {
                    let running = match &p.thread {
                        Some(t) => hub.store.thread(t).await?.is_some_and(|t| !t.archived && t.status == ThreadStatus::Running),
                        None => false,
                    };
                    crate::problems::vakten::may_try_again(id, &detail.activity, running)?;
                }
            }
            None => crate::problems::vakten::may_fix(hub, id, again).await?,
        }
    }
    // The contract goes in the problem's note, where the user reads it and the fix thread gets it.
    if note != before {
        let patch = crate::problems::ProblemPatch { note: Some(note), why: Some("the fix contract".into()), ..Default::default() };
        match &host {
            Some(h) => {
                let path = format!("/api/problems/guard/{id}/change");
                hub.remotes.post_to::<Value>(h, &path, &json!({ "by": by, "patch": patch }), REMOTE_WAIT).await?;
            }
            None => {
                crate::problems::change(hub, id, patch, &by).await?;
            }
        }
    }
    // The fix thread starts on the machine the problem is on, where its code is.
    let started: crate::problems::ProblemThread = match &host {
        Some(h) => {
            let path = format!("/api/problems/guard/{id}/fix");
            hub.remotes.post_to(h, &path, &json!({ "by": by, "plan": args["plan"].as_str(), "size": args["size"].as_str(), "again": again }), REMOTE_WAIT).await?
        }
        None => crate::problems::start_thread(hub, id, &by, args["plan"].as_str(), args["size"].as_str()).await?,
    };
    // The machine the problem is on knows no folder for it: start it here, in the project of the
    // same name (Hub's own code is often only on this machine).
    let (started, host) = match (started.thread.is_none(), host) {
        (true, Some(h)) => (crate::problems::fix_here(hub, &h, id, &by, args["plan"].as_str(), args["size"].as_str()).await?, None),
        (_, host) => (started, host),
    };
    match (started.thread, host) {
        (Some(t), None) => Ok(json!({ "thread": t.id, "link": link("thread", &t.id) })),
        (Some(t), Some(h)) => Ok(json!({ "thread": t.id, "machine": h })),
        (None, _) => bail!("Hub does not know the folder of #{id}'s project; tell the user in the note which folder it needs"),
    }
}
