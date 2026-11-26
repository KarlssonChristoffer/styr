//! Live pages an agent shows in its thread: a chart, a mockup, an animation, a small tool. The
//! agent hands Hub a whole HTML document (`show_page`, `mcp/pages.rs`); Hub keeps it as a file under the
//! data folder, logs a `page` event, and serves it at `/api/threads/<id>/pages/<page>` for the
//! thread view to frame.
//!
//! A page runs the agent's scripts, so it is kept away from everything else. It is sent with a
//! `sandbox` policy and never `allow-same-origin`, which gives it an opaque origin: no cookies, no
//! storage, no Tauri bridge, and the API refuses it (its origin is not the app's). It may not
//! fetch or open sockets (`connect-src 'none'`), so it can draw and animate but not send anything
//! anywhere or reach services on this machine. Scripts load inline or from a few CDNs. The only
//! way out is `postMessage` to the thread view, which takes its height, a link to open in the
//! browser, and nothing else.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use regex::Regex;

/// The most a page may be as the agent sends it.
pub const MAX_HTML: usize = 512 * 1024;
/// The most a page may grow to once local images are inlined.
const MAX_INLINED: usize = 8 * 1024 * 1024;
/// The most an HTML file on disk may be to be run as a page (a chart with its library inline).
const MAX_FILE: u64 = 4 * 1024 * 1024;
/// The thread id pages made from files are kept under. They belong to no thread; they are served
/// by the same route and pruned after a day.
pub const FILES: &str = "files";

/// Sent with every page. See the module's comment.
pub const CSP: &str = "sandbox allow-scripts; \
    default-src 'none'; \
    script-src 'unsafe-inline' 'unsafe-eval' blob: https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com; \
    style-src 'unsafe-inline' https://cdnjs.cloudflare.com https://cdn.jsdelivr.net https://unpkg.com https://fonts.googleapis.com; \
    font-src data: https://fonts.gstatic.com https://cdnjs.cloudflare.com https://cdn.jsdelivr.net; \
    img-src data: blob:; media-src data: blob:; worker-src blob:; \
    connect-src 'none'; form-action 'none'; base-uri 'none'";

/// Put first in every page: reports the page's height to the thread view, takes Hub's colours
/// from it, and hands links to it to open in the browser.
const BRIDGE: &str = r##"<meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><style>:root{color-scheme:dark}html,body{margin:0}body{background:var(--bg,#1a1918);color:var(--text-2,#d3cfc8);font:13.5px/1.55 var(--font,ui-sans-serif,system-ui,sans-serif)}</style><script>(()=>{const p=parent,d=document.documentElement;let h=0;const send=()=>{const n=Math.ceil(Math.max(d.scrollHeight,document.body?document.body.scrollHeight:0));if(n!==h){h=n;p.postMessage({hub:"height",height:n},"*")}};const ro=new ResizeObserver(send);ro.observe(d);addEventListener("DOMContentLoaded",()=>{ro.observe(document.body);send()});addEventListener("load",send);addEventListener("message",e=>{if(e.source!==p||!e.data||e.data.hub!=="theme")return;for(const[k,v]of Object.entries(e.data.vars||{}))if(/^--[\w-]+$/.test(k))d.style.setProperty(k,String(v))});addEventListener("click",e=>{const a=e.target instanceof Element&&e.target.closest("a[href]");if(!a||a.getAttribute("href").startsWith("#"))return;if(/^https?:/.test(a.href)){e.preventDefault();p.postMessage({hub:"open",url:a.href},"*")}},true);p.postMessage({hub:"ready"},"*")})()</script>"##;

/// Hub's kit: ready-made `hk-` classes for what answers are made of (pictures, tables, bars,
/// notes, tabs...), in the look of the rest of the chat, served in every page before the agent's
/// own styles so they can still change anything.
const KIT_CSS: &str = include_str!("pages_kit.css");

/// The few clicks the kit's tabs, segmented switches and checklists need.
const KIT_JS: &str = r##"addEventListener("click",e=>{const t=e.target instanceof Element?e.target:null;if(!t)return;const tab=t.closest(".hk-tabs>*");if(tab&&tab.dataset.tab){const box=tab.closest(".hk-tabs").parentElement;box.querySelectorAll(":scope>.hk-tabs>*").forEach(x=>x.classList.toggle("on",x===tab));box.querySelectorAll(":scope>.hk-panel").forEach(x=>{x.hidden=x.dataset.tab!==tab.dataset.tab});return}const seg=t.closest(".hk-seg>*");if(seg){seg.parentElement.querySelectorAll(":scope>*").forEach(x=>x.classList.toggle("on",x===seg));return}const ck=t.closest(".hk-check>*");if(ck)ck.classList.toggle("done")})"##;

/// What the agent is told about writing pages, part of the tool's description.
pub const GUIDE: &str = "Write one self-contained HTML document: inline <style> and <script>, libraries only \
from cdnjs.cloudflare.com, cdn.jsdelivr.net or unpkg.com. The page cannot fetch, so put the data in it. \
Local images given as absolute paths in src are inlined. It sits in a dark, calm chat column about 720px \
wide: use transparent or var(--bg) backgrounds and Hub's colours as CSS variables with fallbacks: \
--bg #1a1918, --surface #211f1e, --line #2a2826, --text #ece8e2, --text-2 #d3cfc8, --muted #958f88, \
--accent #a5abee, --green #84c29a, --amber #ddaa5e, --red #e5806f, --font, --mono. \
Keep it dense, no big hero headers; animation is welcome. Links open in the user's browser. \
Leave html, body and the outermost element without a background, card, border or banner title: the page sits \
straight on the chat. Let the content set the height: no vh units and no height:100% on html or body; give \
a chart or canvas a fixed pixel height (about 260px). It must also work about 390px wide, since the same \
page shows on the phone. Series colours for charts: --chart-1 to --chart-6 (colour on a chart is not state). An inline SVG chart keeps its shape: never preserveAspectRatio=\"none\" or a stretched width and height; read the container width and draw at it, so text and circles are not squashed. \
Hub's kit styles the usual parts for you, so use these classes instead of writing the CSS: \
.hk-cap (a small grey label above a block), .hk-media (a picture, with .hk-chip for its source), \
.hk-gallery, .hk-title, .hk-meta, .hk-card, .hk-table (class n on numeric cells), \
.hk-bars (a div per row: label, <i style=\"--w:60%\">, <b>value</b>), .hk-note (+ .info .err .ok, <b> as its title), \
.hk-stats > .hk-stat (small, b, em), .hk-check (click to tick), .hk-tabs (data-tab on each tab, \
.hk-panel with the same data-tab), .hk-seg, .hk-btn. Colour only means state. \
Plain answers (text, a list, a table, a picture) are better as Markdown in the reply, not a page. \
Check a longer page with page_check first, at the phone width (390) as well as the default.";

/// What every agent is told Hub can show as what it is, and what to do about the rest.
pub const SHOWS: &str = "What the chat shows as what it is: Markdown, pictures (png, jpg, gif, webp, avif), SVG, \
PDF, sound, video, JSON, CSV and TSV tables, Excel and OpenDocument spreadsheets (one table per sheet), notebooks, logs and code. An HTML file you save runs as a live page \
in the viewer (scripts work; nothing loads from the network except cdnjs, jsdelivr and unpkg), so a chart saved \
as .html draws itself. Show a chart or diagram as that interactive page straight away, not as a PNG first. \
A file you write whole (an .html page, a CSV or TSV table, an SVG) also appears in the chat by itself, \
so name it in your answer and do not send the user to a browser. Anything else (a Word or PowerPoint file, a Blender file or render, a 3D \
model, an archive) has no viewer yet: show what it holds with show_page (a table, an <img> of the render, a \
canvas, a short summary) and give the file's path as well. Hub notices the missing viewer by itself and builds it \
later, so do not apologise for it or send the user to another program. \
A diagram, flowchart, schema or chart is not a picture: draw it yourself as an SVG file or a live page, never with an \
image model or a side job (those are for artwork and photos). A description, or a screenshot you left on disk, does \
not show it. Never start a server to show a page. A picture to post or send somewhere else is a .png file and its path.";

/// Where thread `thread`'s pages live.
fn dir(data_dir: &Path, thread: &str) -> PathBuf {
    data_dir.join("pages").join(thread)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// Keeps a page for thread `thread` and gives its id. `cwd` is the thread's folder, which
/// relative image paths are read from.
pub fn save(data_dir: &Path, thread: &str, cwd: &Path, html: &str) -> Result<(String, usize)> {
    ensure!(valid_id(thread), "bad thread id");
    let html = html.trim();
    ensure!(!html.is_empty(), "the page is empty");
    ensure!(html.len() <= MAX_HTML, "the page is {} KB; keep it under {} KB", html.len() / 1024, MAX_HTML / 1024);
    let html = inline_images(html, cwd, None)?;
    let id = uuid::Uuid::new_v4().simple().to_string();
    let dir = dir(data_dir, thread);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    std::fs::write(dir.join(format!("{id}.html")), &html).context("saving the page")?;
    Ok((id, html.len()))
}

/// Runs an HTML file of a project as a page, the way `show_page` would, so the chart an agent
/// wrote to disk draws itself in the viewer instead of showing as a still picture. The file is
/// read from `root` (the folder it is in, or the project), its local images, scripts and styles
/// next to it are inlined (nothing outside `root`), and the page is kept under [`FILES`] with a
/// random id, so only the one who asked can open it. Returns that id. `rel` may also be an
/// absolute path, as an agent gives one (`page_check`), as long as it lies inside `root`.
pub fn save_file(data_dir: &Path, root: &Path, rel: &str) -> Result<String> {
    let asked = Path::new(rel.trim());
    let path = if asked.is_absolute() {
        let file = asked.canonicalize().with_context(|| format!("reading {rel}"))?;
        ensure!(file.starts_with(root.canonicalize().context("the folder")?), "{rel} is outside the folder");
        file
    } else {
        crate::project_folder::inside(root, rel.trim())?
    };
    ensure!(
        path.extension().is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm")),
        "{rel} is not an HTML file"
    );
    let size = std::fs::metadata(&path).with_context(|| format!("reading {rel}"))?.len();
    ensure!(size <= MAX_FILE, "{rel} is {} MB; too big to run here", size / 1024 / 1024);
    let bytes = std::fs::read(&path).with_context(|| format!("reading {rel}"))?;
    let html = String::from_utf8_lossy(&bytes);
    let folder = path.parent().unwrap_or(root);
    let html = inline_local_assets(&html, root, folder);
    let html = inline_images(&html, folder, Some(root))?;
    let dir = dir(data_dir, FILES);
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    prune(&dir);
    let id = uuid::Uuid::new_v4().simple().to_string();
    std::fs::write(dir.join(format!("{id}.html")), html.trim()).context("saving the page")?;
    Ok(id)
}

/// Removes pages made from files that are over a day old.
fn prune(dir: &Path) {
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(24 * 3600);
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        if e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t < old) {
            crate::locked::clean_file(e.path());
        }
    }
}

/// Puts a script or stylesheet that sits next to the HTML file into the page itself, since the
/// page is served from somewhere else and could not reach it. Only files inside `root`; remote
/// ones (`https://…`) stay as they are.
fn inline_local_assets(html: &str, root: &Path, folder: &Path) -> String {
    let read = |src: &str| -> Option<String> {
        if src.contains("://") || src.starts_with("data:") || src.starts_with("//") {
            return None;
        }
        let src = src.split(['?', '#']).next().unwrap_or(src);
        let path = folder.join(src.strip_prefix("file:").unwrap_or(src));
        let (real, base) = (path.canonicalize().ok()?, root.canonicalize().ok()?);
        if !real.starts_with(&base) || std::fs::metadata(&real).ok()?.len() > MAX_FILE {
            return None;
        }
        std::fs::read_to_string(real).ok()
    };
    let script = Regex::new(r#"(?is)<script\b([^>]*?)\bsrc\s*=\s*(["'])([^"'<>]+)["']([^>]*)>\s*</script>"#).expect("constant regex");
    let link = Regex::new(r#"(?is)<link\b[^>]*>"#).expect("constant regex");
    let href = Regex::new(r#"(?is)\bhref\s*=\s*["']([^"'<>]+)["']"#).expect("constant regex");
    let out = script.replace_all(html, |c: &regex::Captures| match read(&c[3]) {
        Some(code) => format!("<script{}{}>{}</script>", &c[1], &c[4], code.replace("</script", "<\\/script")),
        None => c[0].to_owned(),
    });
    link.replace_all(&out, |c: &regex::Captures| {
        let tag = &c[0];
        let sheet = tag.to_ascii_lowercase().contains("stylesheet");
        match (sheet, href.captures(tag).and_then(|h| read(&h[1]))) {
            (true, Some(css)) => format!("<style>{}</style>", css.replace("</style", "<\\/style")),
            _ => tag.to_owned(),
        }
    })
    .into_owned()
}

/// Keeps a page only to look at it (`page_check`), not shown in the thread; `forget` it after.
pub fn save_draft(data_dir: &Path, thread: &str, cwd: &Path, html: &str) -> Result<String> {
    let (id, _) = save(data_dir, thread, cwd, html)?;
    let dir = dir(data_dir, thread);
    let draft = format!("check-{id}");
    std::fs::rename(dir.join(format!("{id}.html")), dir.join(format!("{draft}.html"))).context("keeping the draft")?;
    Ok(draft)
}

/// Removes one page, such as a draft that was checked.
pub fn forget(data_dir: &Path, thread: &str, page: &str) {
    if valid_id(thread) && valid_id(page) {
        crate::locked::clean_file(dir(data_dir, thread).join(format!("{page}.html")));
    }
}

/// Whether thread `thread` has a page `page` (drafts do not count).
pub fn exists(data_dir: &Path, thread: &str, page: &str) -> bool {
    valid_id(thread) && valid_id(page) && !page.starts_with("check-") && dir(data_dir, thread).join(format!("{page}.html")).is_file()
}

/// Where this core serves a page to a browser on the same machine (`page_check`).
pub fn local_url(config: &crate::Config, thread: &str, page: &str) -> String {
    format!("http://{}/api/threads/{thread}/pages/{page}", crate::auth::local_addr(config))
}

/// The page as served: Hub's bridge first, then the agent's document.
pub fn load(data_dir: &Path, thread: &str, page: &str) -> Result<String> {
    ensure!(valid_id(thread) && valid_id(page), "no such page");
    let html = std::fs::read_to_string(dir(data_dir, thread).join(format!("{page}.html"))).context("no such page")?;
    Ok(format!("<!doctype html>{BRIDGE}<style>{KIT_CSS}</style><script>{KIT_JS}</script>{}", strip_doctype(&html)))
}

/// Removes every page of a thread, with the thread.
pub fn remove(data_dir: &Path, thread: &str) {
    if valid_id(thread) {
        crate::locked::clean_dir(dir(data_dir, thread));
    }
}

fn strip_doctype(html: &str) -> &str {
    let t = html.trim_start();
    if t.len() >= 9 && t[..9].eq_ignore_ascii_case("<!doctype") {
        t.find('>').map_or(t, |end| &t[end + 1..])
    } else {
        t
    }
}

/// Replaces `src="<local image>"` with the image itself, so the page keeps working when the file
/// moves. Remote and data URLs stay as they are.
/// With a `root`, only pictures inside it are read (a file run as a page, `save_file`); a page the
/// agent hands over itself may show any picture it could read anyway.
fn inline_images(html: &str, cwd: &Path, root: Option<&Path>) -> Result<String> {
    let re = Regex::new(r#"(?i)\bsrc\s*=\s*(["'])([^"'<>]+?\.(png|jpe?g|gif|webp|svg|avif))["']"#).expect("constant regex");
    let base = root.map(|r| r.canonicalize()).transpose()?;
    let mut out = String::with_capacity(html.len());
    let mut last = 0;
    for c in re.captures_iter(html) {
        let (whole, quote, src, ext) = (c.get(0).expect("group 0 always matches"), &c[1], &c[2], c[3].to_ascii_lowercase());
        if src.contains("://") || src.starts_with("data:") || src.starts_with("//") {
            continue;
        }
        let path = cwd.join(src.strip_prefix("file:").unwrap_or(src));
        // A file run as a page only shows pictures beside it, as for its scripts and styles, so an
        // HTML file in a cloned repo cannot carry one from elsewhere (`../`, an absolute path).
        if let Some(base) = &base {
            let Ok(real) = path.canonicalize() else { continue };
            if !real.starts_with(base) {
                continue;
            }
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let mime = match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "gif" => "image/gif",
            "webp" => "image/webp",
            _ => "image/avif",
        };
        out.push_str(&html[last..whole.start()]);
        out.push_str(&format!("src={quote}data:{mime};base64,{}{quote}", base64(&bytes)));
        last = whole.end();
        if out.len() > MAX_INLINED {
            bail!("the page with its images is over {} MB; use smaller images", MAX_INLINED / 1024 / 1024);
        }
    }
    out.push_str(&html[last..]);
    Ok(out)
}

pub(crate) fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn an_html_file_runs_as_a_page_with_its_neighbours_inlined() {
        let data = std::env::temp_dir().join(format!("hub-pages-{}", uuid::Uuid::new_v4().simple()));
        let root = data.join("out");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("chart.js"), "draw(); // </script> inside").unwrap();
        std::fs::write(root.join("look.css"), "body{color:red}").unwrap();
        std::fs::write(data.join("secret.js"), "steal()").unwrap();
        let html = "<!doctype html><link rel=\"stylesheet\" href=\"look.css\"><script src=\"chart.js\"></script>\
            <script src=\"../secret.js\"></script><script src=\"https://cdn.jsdelivr.net/npm/x.js\"></script>";
        std::fs::write(root.join("diagram.html"), html).unwrap();
        let id = save_file(&data, &root, "diagram.html").unwrap();
        let served = load(&data, FILES, &id).unwrap();
        assert!(served.contains("<style>body{color:red}</style>"), "{served}");
        assert!(served.contains("draw(); // <\\/script> inside"), "{served}");
        assert!(!served.contains("steal()") && served.contains("src=\"../secret.js\""), "nothing outside the folder is read");
        assert!(served.contains("src=\"https://cdn.jsdelivr.net/npm/x.js\""), "remote scripts stay remote");
        std::fs::write(data.join("private.png"), [137, 80, 78, 71]).unwrap();
        std::fs::write(root.join("pic.html"), "<img src=\"../private.png\">").unwrap();
        let id = save_file(&data, &root, "pic.html").unwrap();
        assert!(load(&data, FILES, &id).unwrap().contains("src=\"../private.png\""), "no picture from outside the folder");
        assert!(save_file(&data, &root, "../out/diagram.html").is_err(), "no climbing");
        let whole = root.join("diagram.html").display().to_string();
        assert!(load(&data, FILES, &save_file(&data, &root, &whole).unwrap()).unwrap().contains("body{color:red}"), "an absolute path inside the folder");
        let outside = data.join("secret.js").display().to_string();
        assert!(save_file(&data, &root, &outside).is_err(), "an absolute path outside the folder");
        std::fs::write(root.join("note.txt"), "x").unwrap();
        assert!(save_file(&data, &root, "note.txt").is_err(), "only HTML");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn the_guide_and_the_kit_name_the_same_classes() {
        let classes = |text: &str| -> std::collections::BTreeSet<String> {
            Regex::new(r"\.hk-[a-z]+").unwrap().find_iter(text).map(|m| m.as_str().to_owned()).collect()
        };
        let (told, styled) = (classes(GUIDE), classes(KIT_CSS));
        // `.hk-panel` is only a name for the click script; it hides with the `hidden` attribute.
        let missing: Vec<_> = told.iter().filter(|c| !styled.contains(*c) && *c != ".hk-panel").collect();
        assert!(missing.is_empty(), "the guide tells the agent about classes the kit does not style: {missing:?}");
        let unknown: Vec<_> = styled.iter().filter(|c| !told.contains(*c)).collect();
        assert!(unknown.is_empty(), "the kit styles classes the guide never mentions: {unknown:?}");
    }

    #[test]
    fn every_page_has_chart_colours_and_every_agent_is_told_what_a_diagram_is() {
        for n in 1..=6 {
            assert!(KIT_CSS.contains(&format!("--chart-{n}:")), "--chart-{n} comes with the kit");
            assert!(GUIDE.contains("--chart-1 to --chart-6"));
        }
        assert!(SHOWS.contains("A diagram, flowchart, schema or chart is not a picture"));
        assert!(SHOWS.contains("Never start a server"));
    }

    #[test]
    fn pages_are_kept_served_with_the_bridge_and_removed_with_the_thread() {
        let data = std::env::temp_dir().join(format!("hub-pages-{}", uuid::Uuid::new_v4().simple()));
        let cwd = data.join("repo");
        std::fs::create_dir_all(&cwd).unwrap();
        std::fs::write(cwd.join("dot.png"), [137, 80, 78, 71]).unwrap();
        let html = "<!DOCTYPE html><html><body><img src=\"dot.png\"><img src='https://x.test/a.png'></body></html>";
        let (id, _) = save(&data, "t1", &cwd, html).unwrap();
        let served = load(&data, "t1", &id).unwrap();
        assert!(served.starts_with("<!doctype html><meta charset"), "{served}");
        assert_eq!(served.matches("<!").count(), 1, "one doctype: {served}");
        assert!(served.contains("src=\"data:image/png;base64,iVBORw==\""), "{served}");
        assert!(served.contains("src='https://x.test/a.png'"), "remote images stay");
        assert!(served.contains(".hk-media{") && served.contains(".hk-bars i:after{") && served.contains(".hk-tabs>*"), "the kit comes with every page");
        assert!(served.find(".hk-media{").unwrap() < served.find("<img src=").unwrap(), "the agent's own styles come after the kit");
        assert!(load(&data, "t1", "../t1").is_err() && load(&data, "..", &id).is_err());
        assert!(save(&data, "t1", &cwd, &"x".repeat(MAX_HTML + 1)).is_err());
        assert!(exists(&data, "t1", &id) && !exists(&data, "t1", "nope"));
        let draft = save_draft(&data, "t1", &cwd, "<p>draft</p>").unwrap();
        assert!(draft.starts_with("check-") && load(&data, "t1", &draft).is_ok() && !exists(&data, "t1", &draft));
        forget(&data, "t1", &draft);
        assert!(load(&data, "t1", &draft).is_err());
        remove(&data, "t1");
        assert!(load(&data, "t1", &id).is_err());
        let _ = std::fs::remove_dir_all(&data);
    }
}
