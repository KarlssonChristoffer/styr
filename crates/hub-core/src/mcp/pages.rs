//! Live pages in a thread: `show_page` keeps an agent's HTML page and logs it so the thread shows
//! it, and `page_check` opens a draft or a shown page in a headless browser and tells the agent
//! what it looks like. Their declarations (`page_tools`) and what each call does.

use std::sync::Arc;

use serde_json::{Value, json};

use super::{on_thread, read_only};
use crate::hub::Hub;
use crate::hub_tools::tool::{Call, Out, Reply, Tool, text};
use crate::run;

const SHOW_PAGE: &str = "Show a live HTML page inline in this thread: a chart, diagram, table, mockup, \
animation or small interactive tool. Use it when seeing or trying something beats reading about it, or \
when the user asks for something visual. It appears where you call it; don't restate its content \
afterwards. To change a page you showed, call it again with the whole new page and `replaces` set to the \
earlier page's id: it becomes the next version of that page. For a page that is more than a few lines, \
look at it first with page_check.";

const PAGE_CHECK: &str = "Look at a live page before you show it, or at one you showed: Hub opens it in a headless \
browser, as the thread would show it, and answers with a screenshot, its height and any script errors, console \
errors or loads its sandbox blocked. Pass `html` for a draft (nothing is shown in the thread), `file` for an HTML \
file in this thread's folder (the styles, scripts and pictures beside it come along), or `page` for the id of a page \
you showed. Check at the default width and again at about 390 for a phone. Fix what it finds, then show_page.";

/// Showing pages and checking them.
pub(super) fn page_tools(tools: &mut Vec<Tool>) {
    tools.push(Tool::of(json!({
        "name": "show_page",
        "description": format!("{SHOW_PAGE} {}", crate::pages::GUIDE),
        "inputSchema": {
            "type": "object",
            "properties": {
                "html": { "type": "string", "description": "A complete, self-contained HTML document." },
                "title": { "type": "string", "description": "A short name for the page, a few words." },
                "height": { "type": "integer", "description": "Roughly how tall it is in CSS pixels (120 to 1200). The frame fits itself to the page once it loads." },
                "replaces": { "type": "string", "description": "The id of a page you showed in this thread, when this is a new version of it." },
            },
            "required": ["html", "title"],
        },
    }), run!(|c| thread!(c, |t| text(show_page(c.hub, t, c.args).await)))));
    tools.push(Tool::of(json!({
        "name": "page_check",
        "description": PAGE_CHECK,
        "inputSchema": {
            "type": "object",
            "properties": {
                "html": { "type": "string", "description": "A complete page to look at without showing it." },
                "file": { "type": "string", "description": "Or the path of an .html file in this thread's folder, relative to it or absolute." },
                "page": { "type": "string", "description": "Or the id of a page you showed." },
                "width": { "type": "integer", "description": "The width in CSS pixels, 240 to 1920; the chat column is 720, use about 390 for a phone." },
            },
        },
        "annotations": read_only(),
    }), run!(|c| page_check_tool(c).await)));
}

/// Keeps the agent's page and logs it, so the thread view shows it.
async fn show_page(hub: &Arc<Hub>, thread: &crate::model::Thread, args: &Value) -> Result<String, String> {
    let html = args["html"].as_str().unwrap_or_default();
    let title: String = args["title"].as_str().unwrap_or("Page").trim().chars().take(80).collect();
    let height = args["height"].as_i64().map(|h| h.clamp(120, 1200));
    let replaces = args["replaces"].as_str().map(str::trim).filter(|r| !r.is_empty()).map(str::to_owned);
    if let Some(r) = &replaces
        && !crate::pages::exists(&hub.config.data_dir, &thread.id, r)
    {
        return Err(format!("This thread has no page {r} to replace; leave out `replaces` to show a new page."));
    }
    let (data_dir, cwd, id) = (hub.config.data_dir.clone(), std::path::PathBuf::from(&thread.cwd), thread.id.clone());
    let html = html.to_owned();
    let (page, bytes) = tokio::task::spawn_blocking(move || crate::pages::save(&data_dir, &id, &cwd, &html))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))?;
    let data = crate::model::PageData { id: page.clone(), title: title.clone(), height: Some(height), bytes: bytes as u64, replaces };
    hub.emit_typed(&thread.id, &data).await.map_err(|e| format!("{e:#}"))?;
    if let Err(e) = crate::widgets::page_shown(&hub.ctx(), &thread.id, &page, &title).await {
        tracing::warn!(thread = %thread.id, "putting a page on its widget: {e:#}");
    }
    Ok(format!(
        "The page \"{title}\" is showing in the thread and the user sees it: do not describe it or repeat what it shows; \
         reply only with what it does not already say. Its id is {page}."
    ))
}

/// What the engine sees of `page_check`: the numbers and the picture.
async fn page_check_tool(c: &Call<'_>) -> Reply {
    let thread = on_thread(c)?;
    let seen = page_check(c.hub, thread, c.args).await?;
    Ok(Out::Content(vec![
        json!({ "type": "text", "text": seen.summary() }),
        json!({ "type": "image", "data": seen.png, "mimeType": "image/png" }),
    ]))
}

/// Opens a draft or a shown page in a headless browser and tells the agent what it looks like.
async fn page_check(hub: &Arc<Hub>, thread: &crate::model::Thread, args: &Value) -> Result<crate::page_check::Checked, String> {
    let width = args["width"].as_u64().map_or(crate::page_check::DEFAULT_WIDTH, |w| w.clamp(240, 1920) as u32);
    let data_dir = hub.config.data_dir.clone();
    // A file of the project runs the way the viewer runs it, kept with the pages made from files.
    if args["html"].as_str().is_none_or(|h| h.trim().is_empty())
        && let Some(file) = args["file"].as_str().filter(|f| !f.trim().is_empty())
    {
        let (dir, cwd, file) = (data_dir.clone(), std::path::PathBuf::from(&thread.cwd), file.to_owned());
        let page = tokio::task::spawn_blocking(move || crate::pages::save_file(&dir, &cwd, &file))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{e:#}"))?;
        let seen = crate::page_check::check(&crate::pages::local_url(&hub.config, crate::pages::FILES, &page), width).await;
        crate::pages::forget(&data_dir, crate::pages::FILES, &page);
        return seen.map_err(|e| format!("{e:#}"));
    }
    let (page, draft) = match (args["html"].as_str().filter(|h| !h.trim().is_empty()), args["page"].as_str()) {
        (Some(html), _) => {
            let (data_dir, cwd, id, html) = (data_dir.clone(), std::path::PathBuf::from(&thread.cwd), thread.id.clone(), html.to_owned());
            let draft = tokio::task::spawn_blocking(move || crate::pages::save_draft(&data_dir, &id, &cwd, &html))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| format!("{e:#}"))?;
            (draft, true)
        }
        (None, Some(page)) if crate::pages::exists(&data_dir, &thread.id, page.trim()) => (page.trim().to_owned(), false),
        (None, Some(page)) => return Err(format!("This thread has no page {page}.")),
        (None, None) => return Err("Pass the page's html, the path of its file, or the id of a page you showed.".into()),
    };
    let seen = crate::page_check::check(&crate::pages::local_url(&hub.config, &thread.id, &page), width).await;
    if draft {
        crate::pages::forget(&data_dir, &thread.id, &page);
    }
    seen.map_err(|e| format!("{e:#}"))
}
