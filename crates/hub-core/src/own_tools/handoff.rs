//! Handoff of `own_tools.rs`: what a tool is handed and what it answers. The steps that ask an
//! engine or note the handoff in the thread are Hub's (`hub/tool_handoff.rs`).

use super::*;

/// The prompt that has the thread's engine write the brief for a handoff to `manifest`'s tool.
pub(crate) fn draft_prompt(manifest: &Manifest, thread: &crate::model::Thread, log: &str) -> Result<String> {
    let handoff = manifest.handoff.as_ref().with_context(|| format!("{} takes no handoffs", manifest.name))?;
    Ok(format!(
        "{DRAFT}\n\n<guide tool=\"{}\">\n{}\n</guide>\n\nThread: {}\nFolder: {}\n\n<conversation>{log}\n</conversation>",
        manifest.name,
        handoff.guide.trim(),
        thread.title,
        thread.cwd
    ))
}

/// The first line is the title, the rest the brief.
pub(crate) fn split_draft(answer: &str, fallback: &str) -> (String, String) {
    let answer = answer.trim();
    let (first, rest) = answer.split_once('\n').unwrap_or((answer, ""));
    let title = first.trim().trim_start_matches(['#', ' ']).trim_start_matches("Title:").trim().trim_matches(['"', '*']).trim();
    if title.is_empty() || rest.trim().is_empty() {
        return (fallback.to_owned(), answer.to_owned());
    }
    (title.chars().take(100).collect(), rest.trim().to_owned())
}

pub(super) const DRAFT: &str = "Below is a conversation between a person and a coding agent, and a guide from a tool \
that will do the work unattended. Do not continue the conversation and do not use tools. Write the brief \
the guide asks for, for the work the person wants done next, standing on its own: the tool sees nothing \
else. Name files, commands and behaviour exactly, in the person's language. Answer with a short title on \
the first line, then a blank line, then the brief, and nothing else.";

/// The prompt that has the thread's engine answer what the tool asked before it takes the work
/// (how the repository runs its tests…), from the folder's top files.
pub(crate) fn answers_prompt(thread: &crate::model::Thread, asks: &[crate::model::ToolAsk]) -> String {
    let files = std::fs::read_dir(&thread.cwd)
        .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).take(80).collect::<Vec<_>>().join(", "))
        .unwrap_or_default();
    let questions: Vec<String> = asks.iter().map(|a| format!("- {}: {} (example: {})", a.name, a.label, a.hint.as_deref().unwrap_or("none"))).collect();
    format!("{ANSWER}\n\nFolder: {}\nFiles at its top: {files}\n\nQuestions:\n{}", thread.cwd, questions.join("\n"))
}

/// Each ask's answer from what the engine `said`, else the tool's own example.
pub(crate) fn read_answers(asks: &[crate::model::ToolAsk], said: &str) -> BTreeMap<String, String> {
    let doc: Value = match (said.find('{'), said.rfind('}')) {
        (Some(a), Some(b)) if a < b => serde_json::from_str(&said[a..=b]).unwrap_or(Value::Null),
        _ => Value::Null,
    };
    asks.iter()
        .map(|a| {
            let given = doc[&a.name].as_str().map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
            (a.name.clone(), given.or_else(|| a.hint.clone()).unwrap_or_default())
        })
        .collect()
}

pub(super) const ANSWER: &str = "A tool that builds code unattended needs a few facts about a repository before it \
takes work there. Answer each question for the folder below from what its files show, with a command or \
name that would work as is; when you cannot tell, use the example. Do not use tools. Answer with one JSON \
object, each question's name to its answer as a string, and nothing else.";

/// The card that offers the user a handoff to the tool in `folder`, with the agent's reason.
pub(crate) async fn offer(ctx: &Ctx<'_>, folder: &str, why: &str) -> Result<HandoffOfferData> {
    let (dir, manifest) = found(ctx, folder).await?;
    let handoff = manifest.handoff.with_context(|| format!("{} takes no handoffs", manifest.name))?;
    Ok(HandoffOfferData {
        tool: manifest.name.clone(),
        folder: dir.display().to_string(),
        label: handoff.label.clone(),
        about: Some(handoff.about.clone()),
        can_build: handoff.build.is_some(),
        why: why.to_owned(),
    })
}

/// Hands the thread's work to the tool with the brief the user approved. With what ran, the
/// `handed_off` to note in the thread, or none when the tool first asks something
/// (`OwnToolRan::asks`).
pub(crate) async fn hand_off(ctx: &Ctx<'_>, thread_id: &str, approved: &HandoffBrief) -> Result<(OwnToolRan, Option<HandedOffData>)> {
    let (dir, manifest) = found(ctx, &approved.folder).await?;
    let handoff = manifest.handoff.as_ref().with_context(|| format!("{} takes no handoffs", manifest.name))?;
    let thread = ctx.store.thread(thread_id).await?.context("no such thread")?;
    let (title, brief) = (approved.title.trim(), approved.brief.trim());
    if title.is_empty() || brief.is_empty() {
        bail!("the title and the brief are needed");
    }
    let file = ctx.config.data_dir.join("handoffs").join(format!("{}.md", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(file.parent().expect("has a parent"))?;
    std::fs::write(&file, format!("{brief}\n"))?;
    let inputs = HashMap::from([
        ("repo".to_owned(), thread.cwd.clone()),
        ("title".to_owned(), title.to_owned()),
        ("work".to_owned(), format!("hub:thread:{}", thread.id)),
        ("brief_file".to_owned(), file.display().to_string()),
    ]);
    let mut run = handoff.run.clone();
    if approved.build
        && let Some(build) = &handoff.build
    {
        run.extend(build.iter().cloned());
    }
    // Whole arguments added after the command line is filled, so an answer is never read as
    // `{input}` or parsed.
    let mut answers = Vec::new();
    for (name, value) in &approved.answers {
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            bail!("{name:?} is not something the tool asked for");
        }
        if !value.trim().is_empty() {
            answers.push(format!("--{}", name.replace('_', "-")));
            answers.push(value.trim().to_owned());
        }
    }
    let out = command(&dir, &manifest.env, &run, &inputs)?.args(&answers).output();
    let out = tokio::time::timeout(TIMEOUT * 10, out).await.with_context(|| format!("{} took too long", handoff.label))??;
    READINGS.lock().unwrap_or_else(|e| e.into_inner()).remove(&dir);
    if handoff.needs_exit.is_some() && out.status.code() == handoff.needs_exit {
        let doc: Value = serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).unwrap_or(Value::Null);
        let asks: Vec<crate::model::ToolAsk> = serde_json::from_value(doc["ask"].clone()).unwrap_or_default();
        let output = doc["said"].as_str().map(str::to_owned).unwrap_or_else(|| said(&out));
        return Ok((OwnToolRan { ok: false, output, asks }, None));
    }
    let ran = OwnToolRan { ok: out.status.success(), output: said(&out), asks: Vec::new() };
    let handed = HandedOffData {
        tool: manifest.name.clone(),
        title: title.to_owned(),
        ok: ran.ok,
        output: ran.output.clone(),
        folder: Some(approved.folder.clone()),
        work: Some(inputs["work"].clone()),
    };
    Ok((ran, Some(handed)))
}
