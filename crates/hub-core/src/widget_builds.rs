//! Building an Overview widget (`widgets.rs`): starts the thread that builds it, with the brief
//! that tells the agent what a widget page is, and sends that thread the user's change. Those need
//! the running Hub; the list and what a finished turn or a shown page does to it are `widgets.rs`.

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::hub::Hub;
use crate::model::{NewThread, Widget, WidgetState, now_ms};
use crate::widgets;

/// The longest ask kept, in characters.
const MAX_ASK: usize = 2_000;

/// Starts the thread that builds a widget, and lists it as building.
pub async fn create(hub: &Arc<Hub>, ask: &str) -> Result<Widget> {
    if hub.halted() {
        bail!("every agent is paused; resume them first");
    }
    let ask: String = ask.trim().chars().take(MAX_ASK).collect();
    if ask.is_empty() {
        bail!("say what the widget should show");
    }
    let id = format!("w{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    let folder = hub.config.data_dir.join("widgets").join(&id);
    std::fs::create_dir_all(&folder).with_context(|| format!("making {}", folder.display()))?;
    let title = widgets::title_of(&ask);
    let thread = hub.create_thread_as(
        NewThread {
            cwd: folder.to_string_lossy().into_owned(),
            title: Some(format!("Widget · {title}")),
            engine: None,
            message: None,
            attachments: Vec::new(),
            settings: Default::default(),
            original: None,
            client_id: None,
            attempts: Vec::new(),
            warm: None,
            design: false,
            unattended: None,
            reach: None,
            cloud: false,
        },
        None,
        Some(crate::model::Owner::new(crate::model::OwnerKind::Widget, "Widget", Some(id.clone()))),
    )
    .await?;
    let widget = Widget {
        id: id.clone(),
        title: title.clone(),
        ask: ask.clone(),
        thread: Some(thread.id.clone()),
        page: None,
        state: WidgetState::Building,
        error: String::new(),
        created: now_ms(),
    };
    widgets::add(&hub.ctx(), widget.clone()).await?;
    let mark = json!({ "id": id, "title": title, "ask": ask });
    if let Err(e) = hub.send_from(&thread.id, brief(&ask), Vec::new(), None, Some(("widget", mark)), None).await {
        let error = format!("Could not start: {e:#}");
        widgets::change(&hub.ctx(), |w| w.id == id, |w| {
            w.state = WidgetState::Failed;
            w.error = error.clone();
        })
        .await?;
        bail!(error);
    }
    Ok(widget)
}

/// Asks the widget's thread for a change; the card shows building until the new page is in.
pub async fn ask_change(hub: &Arc<Hub>, id: &str, ask: &str) -> Result<Widget> {
    let ask: String = ask.trim().chars().take(MAX_ASK).collect();
    if ask.is_empty() {
        bail!("say what to change");
    }
    let widget = widgets::list(&hub.ctx()).await?.into_iter().find(|w| w.id == id).context("no such widget")?;
    let thread = widget.thread.clone().context("this widget has no thread to ask")?;
    let page = widget.page.clone();
    let text = match &page {
        Some(page) => format!(
            "(Note from Hub: the user wants this change to the Overview widget. Show the whole page again with \
show_page, `replaces: \"{page}\"`, the same way as before.)\n\n{ask}"
        ),
        None => format!("(Note from Hub: the widget has no page yet. Build it with this in mind, then show it with show_page.)\n\n{ask}"),
    };
    let mark = json!({ "id": id, "title": widget.title, "ask": ask, "change": true });
    hub.send_from(&thread, text, Vec::new(), None, Some(("widget", mark)), None).await?;
    widgets::change(&hub.ctx(), |w| w.id == id, |w| {
        w.state = WidgetState::Building;
        w.error.clear();
    })
    .await?;
    widgets::list(&hub.ctx()).await?.into_iter().find(|w| w.id == id).context("no such widget")
}

/// What the building agent is told: what a widget is, how it gets Hub's numbers, and the ask.
pub fn brief(ask: &str) -> String {
    format!(
        "(Note from Hub: the user asked for a widget on Hub's Overview page, a board of cards. Build it as ONE live \
page and show it with show_page; that page is the card, and it appears on the board the moment you show it. \
Don't make files in this folder, install anything or open pull requests; the page is the whole job.

How a widget page works:
- Self-contained HTML with inline <style> and <script>; libraries only from the CDNs show_page allows. It cannot \
fetch anything or keep anything between loads.
- It sits in a card that the user sizes, from about 300 to 1500 px wide; its height follows the content (keep it \
roughly 120 to 500 px), so lay it out to fit any width. No outer margin, frame or background of its own: the card \
draws those. Use Hub's colours as CSS variables, which are set on the page: --text, --text-2, --muted, --line, \
--accent, --green, --amber, --red (green/amber/red only for good/warning/bad), --surface-hover. Calm, dense, dark; \
one heading in 14.5px semibold --text like the other cards, numbers at 18px.
- Live data from Hub: post `parent.postMessage({{hub: \"data\", want}}, \"*\")` and listen for a message whose \
data is `{{hub: \"data\", want, data}}`. Ask on load and again every minute if it changes over time. `want` is one of:
  - \"pulse\": {{ moments: [ms when the user wrote to an agent], pulls: [{{date: \"YYYY-MM-DD\", count}}] (pull \
requests opened per day), longest_thread: {{title, turns}} | null }} over the last year
  - \"lines\": {{ days: [{{date, added, removed, commits}}] }} lines the user's commits added and removed per day
  - \"stats\": {{ days, answers, tokens, cost, models: [{{model, answers, tokens, cost}}], projects: [{{name, \
tokens, cost}}] }} for the last 30 days (cost in dollars at API prices)
  - \"threads\": [{{title, status (\"running\", \"waiting_on_you\", \"idle\", \"error\"), project, updated (ms)}}] \
the user's agent threads
  - \"now\": {{ time: ms, tz: minutes east of UTC }}
  Draw a calm empty state while the answer is on its way, and when a list is empty. Anything else (weather, \
prices, news) cannot be fetched: put a snapshot of what you know into the page and say it is a snapshot, or say \
what it would need.
- Check it with page_check before you show it (try a narrow and a wide width), fix what it finds, then show_page \
with a short title for the card. Reply in one or two sentences on what the widget shows.)

{ask}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_brief_carries_the_ask_and_the_bridge() {
        let b = brief("my open PRs as dots");
        assert!(b.ends_with("my open PRs as dots"));
        assert!(b.contains("{hub: \"data\", want}"));
        assert!(b.contains("show_page"));
    }
}
