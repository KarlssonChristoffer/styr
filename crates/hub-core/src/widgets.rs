//! Widgets: Overview cards an agent writes on request. The user describes one ("a countdown to
//! Friday 17:00 and my open PRs as dots") and Hub starts an ordinary thread on the subscription
//! engine, marked `widget: { id, title }`, in a folder of its own under the data folder. The agent
//! writes the card as a live page with `show_page` (`pages.rs`), so it runs in the same sandbox as
//! every page: scripts, but no network, no storage and no way into Hub. The Overview frames that
//! page as the card (`WidgetCard.svelte`) and answers the page's asks for numbers it already has
//! (`{hub: "data", want}`), so a widget shows live data without any key or fetch of its own.
//!
//! Nothing is built into the app, so a widget appears the moment its page is shown, with no
//! rebuild or restart. A change ("make the dots bigger") is a message to the same thread, which
//! shows a new version of the page; the card follows the newest page the thread showed.
//!
//! The list is kept under `widgets`. This file keeps the list and what a turn or a shown page does
//! to it (`&Ctx`); starting the build and asking for a change send messages, so they are
//! `widget_builds.rs`.

use std::sync::LazyLock;

use anyhow::Result;
use tokio::sync::Mutex;

use crate::ctx::Ctx;
use crate::model::{Event, EventKind, Widget, WidgetState};

const KEY: &str = "widgets";

/// One writer at a time, since a page being shown and the user asking can meet.
static LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

async fn load(ctx: &Ctx<'_>) -> Result<Vec<Widget>> {
    Ok(match ctx.store.get_value(KEY).await? {
        Some(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        None => Vec::new(),
    })
}

async fn save(ctx: &Ctx<'_>, all: &[Widget]) -> Result<()> {
    ctx.store.set_value(KEY, &serde_json::to_string(all)?).await
}

/// Changes the widgets that `pick` chooses, under the lock; true when any was.
pub(crate) async fn change(ctx: &Ctx<'_>, pick: impl Fn(&Widget) -> bool, f: impl Fn(&mut Widget)) -> Result<bool> {
    let _guard = LOCK.lock().await;
    let mut all = load(ctx).await?;
    let mut any = false;
    for w in all.iter_mut().filter(|w| pick(w)) {
        f(w);
        any = true;
    }
    if any {
        save(ctx, &all).await?;
    }
    Ok(any)
}

pub async fn list(ctx: &Ctx<'_>) -> Result<Vec<Widget>> {
    load(ctx).await
}

/// Lists a new widget, under the lock (`widget_builds::create`).
pub(crate) async fn add(ctx: &Ctx<'_>, widget: Widget) -> Result<()> {
    let _guard = LOCK.lock().await;
    let mut all = load(ctx).await?;
    all.push(widget);
    save(ctx, &all).await
}

/// Takes a widget off the list. Its thread stays, like any thread.
pub async fn delete(ctx: &Ctx<'_>, id: &str) -> Result<()> {
    let _guard = LOCK.lock().await;
    let mut all = load(ctx).await?;
    all.retain(|w| w.id != id);
    save(ctx, &all).await
}

/// A thread showed a page: if it builds a widget, that page is the card now.
pub async fn page_shown(ctx: &Ctx<'_>, thread: &str, page: &str, title: &str) -> Result<()> {
    let (page, title) = (page.to_owned(), title.trim().to_owned());
    change(ctx, |w| w.thread.as_deref() == Some(thread), |w| {
        w.page = Some(page.clone());
        w.state = WidgetState::Ready;
        w.error.clear();
        if !title.is_empty() && title != "Page" {
            w.title = title.chars().take(60).collect();
        }
    })
    .await
    .map(|_| ())
}

/// After a turn ends or fails in a widget's thread: still building means no page came, so the
/// widget failed, unless it already had a page (a change that came to nothing keeps the old one).
pub(crate) async fn finished(ctx: &Ctx<'_>, event: &Event) -> Result<()> {
    let thread = event.thread_id.as_str();
    let building = load(ctx).await?.into_iter().any(|w| w.thread.as_deref() == Some(thread) && w.state == WidgetState::Building);
    if !building {
        return Ok(());
    }
    let why = match event.kind {
        EventKind::TurnEnd if event.ended_well() => {
            let events = ctx.store.events_after_last(thread, EventKind::UserMessage, &[]).await?;
            let reply = crate::model::outcome(&events).reply;
            if reply.trim().is_empty() { "The agent finished without showing a page.".to_owned() } else { reply }
        }
        EventKind::TurnEnd => "Stopped before it was done.".to_owned(),
        _ => event.failure().unwrap_or("The engine failed.").to_owned(),
    };
    let why: String = why.trim().chars().take(600).collect();
    change(ctx, |w| w.thread.as_deref() == Some(thread) && w.state == WidgetState::Building, |w| {
        if w.page.is_some() {
            w.state = WidgetState::Ready;
        } else {
            w.state = WidgetState::Failed;
            w.error = why.clone();
        }
    })
    .await
    .map(|_| ())
}

/// True for a widget id: `w` and ten hex digits.
pub fn is_id(s: &str) -> bool {
    s.len() == 11 && s.starts_with('w') && s[1..].bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// A title from the ask: its first words.
pub(crate) fn title_of(ask: &str) -> String {
    let first = ask.lines().next().unwrap_or(ask).trim();
    let mut out = String::new();
    for word in first.split_whitespace() {
        if out.chars().count() + word.chars().count() > 40 {
            out.push('…');
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_the_first_words() {
        assert_eq!(title_of("A countdown to Friday"), "A countdown to Friday");
        assert_eq!(title_of("A countdown to Friday 17:00 that turns orange the last hour, plus PRs"), "A countdown to Friday 17:00 that turns…");
        assert_eq!(title_of("Weather\nand more"), "Weather");
    }

    #[test]
    fn widget_ids_are_known() {
        assert!(is_id("w26366e9e71"));
        assert!(!is_id("widgets"));
        assert!(!is_id("w26366e9e7"));
    }
}
