//! The phone page: Hub on the phone over the tailnet. Settings > Access shows a QR code that opens
//! `https://<machine>.<tailnet>.ts.net/phone` with a one-time code; the phone trades it for a token
//! of its own (`pairing.rs`). That token only follows threads, answers them and stops them, and only
//! from the tailnet: `tailscale serve` hands requests to hubd on loopback with the phone's tailnet
//! address in `X-Forwarded-For`, and Funnel's (from the internet) are turned away.
//!
//! The page is one file of plain HTML and script with no build step, so every hubd has it, also on
//! machines that never build the app.

use std::net::{IpAddr, SocketAddr};

use axum::extract::ConnectInfo;
use axum::extract::Request;
use axum::http::{HeaderMap, Method, header};
use axum::response::{IntoResponse, Response};

/// Whether the request came from the tailnet or this machine, put on every request by the guard.
#[derive(Debug, Clone, Copy)]
pub struct Tailnet(pub bool);

/// Who sent the request, as far as this machine can tell: the peer's address, or the device
/// `tailscale serve` names last in `X-Forwarded-For`. Pairing counts wrong codes by it.
#[derive(Debug, Clone, Copy)]
pub struct Sender(pub Option<IpAddr>);

/// Where the page lives.
pub const PATH: &str = "/phone";

const PAGE: &str = include_str!("phone/index.html");
const ICON: &str = include_str!("phone/icon.svg");
// iOS ignores SVG for the home-screen icon and wants a PNG.
const ICON_180: &[u8] = include_bytes!("phone/icon-180.png");
const ICON_512: &[u8] = include_bytes!("phone/icon-512.png");
const MANIFEST: &str = r##"{"name":"Hub","short_name":"Hub","start_url":"/phone","scope":"/phone","display":"standalone","background_color":"#1a1918","theme_color":"#1a1918","icons":[{"src":"/phone/icon.svg","sizes":"any","type":"image/svg+xml","purpose":"any"},{"src":"/phone/icon-180.png","sizes":"180x180","type":"image/png"},{"src":"/phone/icon-512.png","sizes":"512x512","type":"image/png"}]}"##;

/// The page and its small files. Anything else under `/phone` is not found.
pub async fn serve(request: Request) -> Response {
    let (body, kind) = match request.uri().path() {
        "/phone" | "/phone/" => (PAGE.as_bytes(), "text/html; charset=utf-8"),
        "/phone/icon.svg" => (ICON.as_bytes(), "image/svg+xml"),
        "/phone/icon-180.png" => (ICON_180, "image/png"),
        "/phone/icon-512.png" => (ICON_512, "image/png"),
        "/phone/manifest.webmanifest" => (MANIFEST.as_bytes(), "application/manifest+json"),
        _ => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };
    (
        [
            (header::CONTENT_TYPE, kind),
            (header::CACHE_CONTROL, "no-cache"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::X_FRAME_OPTIONS, "DENY"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src 'self' data:; \
                 connect-src 'self' https://*.ts.net wss://*.ts.net; manifest-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
            ),
        ],
        body,
    )
        .into_response()
}

/// Whether a path is the page.
pub fn is_page(path: &str) -> bool {
    path == PATH || path.starts_with("/phone/")
}

/// The calls a phone's token may make: read threads and their logs, write to a thread, answer its
/// permission questions, stop it, and the live stream.
pub fn may(method: &Method, path: &str) -> bool {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    match (method.as_str(), parts.as_slice()) {
        ("GET", ["api", "health"]) | ("GET", ["api", "threads"]) | ("GET", ["api", "stream"]) | ("GET", ["api", "halt"]) => true,
        // The panic button, so agents can be paused from wherever you are.
        ("POST", ["api", "halt", "pause" | "resume"]) => true,
        // An idea, kept from wherever you are; it only goes into a focus file.
        ("POST", ["api", "ideas"]) => true,
        // The other machines, so the phone follows each one itself while this one sleeps.
        ("POST", ["api", "phone", "machines"]) => true,
        // What the iPhone app shows and picks from: machines, plan limits, projects, and a new
        // ordinary thread (`api::phone_thread`, never the whole `NewThread`).
        ("GET", ["api", "hosts" | "usage" | "projects" | "host" | "project"]) => true,
        // A machine's rings and its Ready for work list, a project's focus and how far it is.
        ("GET", ["api", "host", "about"]) | ("GET", ["api", "machine", "ready"]) => true,
        // A Mac's iPhone simulator: its picture, and starting it or the live picture.
        ("GET", ["api", "simview"]) | ("GET", ["api", "simview", "frame"]) | ("POST", ["api", "simview", "live" | "boot" | "input" | "app" | "shutdown"]) => true,
        ("POST", ["api", "project", "focus", "summaries"]) => true,
        // Ticking a box in FOCUS.md from the sofa (`api::phone_focus`), nothing else of a focus edit.
        ("POST", ["api", "phone", "focus"]) => true,
        // The thread menu: rename, engine, pin, snooze, settle, archive (the whole patch), its
        // agent's mode, its queue, and moving it to another machine.
        ("PATCH", ["api", "threads", id]) => !id.is_empty(),
        ("POST", ["api", "threads", id, "settings"]) => !id.is_empty(),
        ("PATCH" | "DELETE", ["api", "threads", id, "queue", seq]) => !id.is_empty() && !seq.is_empty(),
        ("POST", ["api", "threads", id, "queue", seq, "steer"]) => !id.is_empty() && !seq.is_empty(),
        ("GET", ["api", "threads", id, "aside"]) => !id.is_empty(),
        // Reactions: an emoji on a message, and the user's own emojis and most used ones.
        ("POST", ["api", "threads", id, "react"]) => !id.is_empty(),
        ("GET", ["api", "emoji"]) => true,
        ("PUT" | "DELETE", ["api", "emoji", name]) => !name.is_empty(),
        ("POST", ["api", "threads", "move"]) => true,
        // Settings from the phone: safety and when threads settle, versions and updating hubd.
        ("GET" | "PUT", ["api", "prefs", "permissions" | "threads" | "chatdetail"]) => true,
        ("GET", ["api", "updates"]) => true,
        ("POST", ["api", "hosts", name, "update" | "reconnect"]) => !name.is_empty(),
        // A thread's pull request: read it, its checks, and merge, approve or comment.
        ("GET", ["api", "github", "pulls" | "pull" | "watches" | "card"]) => true,
        ("POST", ["api", "github", "action"]) => true,
        // A project's library: what its threads made, pictures shown as they are (only pictures
        // come back raw, `api::output_raw`), docs as text.
        ("GET", ["api", "project", "outputs"]) | ("GET", ["api", "project", "outputs", "raw" | "text"]) => true,
        // Adding a project folder that is already on the machine.
        ("POST", ["api", "projects"]) => true,
        // Its own notices through ntfy (`ntfy.rs`): which, where, and a test.
        ("GET" | "PUT", ["api", "phone", "notify"]) | ("POST", ["api", "phone", "notify", "test"]) => true,
        // What the app has on screen, so no notice comes about it (`ntfy::held_back`).
        ("POST", ["api", "phone", "looking"]) => true,
        ("POST", ["api", "phone", "threads"]) => true,
        // Picking provider, model and effort before a thread starts: the engines this machine can
        // start, and starting the chosen one while the page is open so it can say what it offers.
        ("GET", ["api", "phone", "engines"]) | ("POST", ["api", "phone", "warm"]) | ("POST", ["api", "phone", "bring"]) => true,
        // What is going on right now, small enough for a widget or a lock screen (`now.rs`).
        ("GET", ["api", "phone", "now"]) => true,
        // A thread's card in the iPhone app's inbox: its last reply, quick replies, what it waits for.
        // What the thread changed, file by file with its diff, to review from the phone.
        ("GET", ["api", "threads", id]) | ("GET", ["api", "threads", id, "events" | "peek" | "changes"]) => !id.is_empty(),
        // The thread's git line: where its branch stands, a drafted commit message, and committing
        // and pushing it. Not `switch`: moving the checkout stays on the machine.
        ("GET", ["api", "threads", id, "gitstate" | "branches"]) | ("POST", ["api", "threads", id, "gitdraft" | "ship"]) => !id.is_empty(),
        ("GET", ["api", "github", "diff"]) => true,
        // A project's files, read only (`phone_files.rs`): its own routes, which refuse folders that
        // are not projects, paths that leave the project and what looks like a secret. The
        // desktop's `/api/project/file` stays closed.
        ("GET", ["api", "phone", "files", "tree" | "file" | "raw" | "search"]) => true,
        // The parts of a project's page (`project_parts.rs`): Git and Docs are read only; the
        // backlog and the board are two markdown files in the project, changed by these calls only.
        ("GET", ["api", "phone", "project", "git" | "docs" | "backlog" | "board"]) => true,
        ("POST", ["api", "phone", "project", "backlog" | "board"]) => true,
        // A picture the thread's agent named by path in its answer (`phone_files::thread_picture`):
        // only a picture inside that thread's own folder.
        ("GET", ["api", "phone", "threads", id, "picture"]) => !id.is_empty(),
        // An HTML file it named, run as a live page (`phone_files::thread_html`), same limits.
        ("POST", ["api", "phone", "threads", id, "page"]) => !id.is_empty(),
        // What Hub watches: the problems it found (a number, not `sources` or `rules`), starting a
        // fix thread for one, and the routines with a run now.
        ("GET", ["api", "problems"]) | ("GET", ["api", "routines"]) => true,
        // Usage over time and the devices let in (names and when they last called; revoking one
        // stays on the machine).
        ("GET", ["api", "stats"]) | ("GET", ["api", "devices"]) => true,
        ("GET" | "PATCH", ["api", "problems", id]) => id.parse::<i64>().is_ok(),
        ("POST", ["api", "problems", id, "thread"]) => id.parse::<i64>().is_ok(),
        ("POST", ["api", "routines", id, "run" | "enabled"]) => !id.is_empty(),
        // Settle, snooze, pin, archive, mark seen: the inbox's own changes, nothing else (`api::tidy_thread`).
        ("POST", ["api", "threads", id, "messages" | "cancel" | "tidy"]) => !id.is_empty(),
        ("POST", ["api", "threads", id, "permissions", request]) => !id.is_empty() && !request.is_empty(),
        _ => false,
    }
}

/// Whether the request comes from the tailnet or from this machine. `tailscale serve` connects
/// from loopback and names the device in `X-Forwarded-For`, adding it last, so that entry is the
/// one to trust; a request with none on loopback is this machine's own.
/// Whether a request came from anywhere but this machine itself: from another address, or through
/// a proxy on this machine such as `tailscale serve`, which says so in `X-Forwarded-For` (or
/// `Forwarded`) and Tailscale's own headers.
pub fn from_elsewhere(request: &Request) -> bool {
    let headers = request.headers();
    let proxied = ["x-forwarded-for", "forwarded", "tailscale-user-login", "tailscale-funnel-request"]
        .iter()
        .any(|name| headers.contains_key(*name));
    let remote = request.extensions().get::<ConnectInfo<SocketAddr>>().is_some_and(|ConnectInfo(peer)| !peer.ip().is_loopback());
    proxied || remote
}

pub fn via_tailnet(request: &Request) -> bool {
    let Some(ConnectInfo(peer)) = request.extensions().get::<ConnectInfo<SocketAddr>>() else { return false };
    from_tailnet(peer.ip(), request.headers())
}

/// The address a request comes from (see `Sender`).
pub fn sender(request: &Request) -> Option<IpAddr> {
    let ConnectInfo(peer) = request.extensions().get::<ConnectInfo<SocketAddr>>()?;
    if !peer.ip().is_loopback() {
        return Some(peer.ip());
    }
    let forwarded = request.headers().get_all("x-forwarded-for").iter().filter_map(|v| v.to_str().ok()).next_back();
    Some(forwarded.and_then(|list| list.rsplit(',').next()?.trim().parse().ok()).unwrap_or(peer.ip()))
}

fn from_tailnet(peer: IpAddr, headers: &HeaderMap) -> bool {
    // Funnel brings the internet in through the same `serve`; never that.
    if headers.contains_key("tailscale-funnel-request") {
        return false;
    }
    if tailnet_ip(peer) {
        return true;
    }
    if !peer.is_loopback() {
        return false;
    }
    let forwarded: Vec<_> = headers.get_all("x-forwarded-for").iter().filter_map(|v| v.to_str().ok()).collect();
    match forwarded.last().and_then(|list| list.rsplit(',').next()) {
        None => true,
        Some(last) => last.trim().parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback() || tailnet_ip(ip)),
    }
}

/// Tailscale's addresses: 100.64.0.0/10 and fd7a:115c:a1e0::/48.
fn tailnet_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            a == 100 && (64..128).contains(&b)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return tailnet_ip(IpAddr::V4(v4));
            }
            let s = v6.segments();
            s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0
        }
    }
}

/// Whether a browser's request comes from the phone page served by this hubd: its origin is the
/// address it asked for, and that address is this machine or its tailnet name. A page elsewhere
/// that points its own name at this machine (DNS rebinding) is neither.
pub fn own_page(headers: &HeaderMap) -> bool {
    let origin = headers.get(header::ORIGIN).and_then(|o| o.to_str().ok());
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok());
    let (Some(origin), Some(host)) = (origin, host) else { return false };
    let same = origin.strip_prefix("https://").or_else(|| origin.strip_prefix("http://")).is_some_and(|o| o.eq_ignore_ascii_case(host));
    same && local_name(host)
}

/// Whether a browser's request comes from the phone page of another machine on the same tailnet:
/// the page on `pc.tail1234.ts.net` following `clanker.tail1234.ts.net`. It still needs a phone
/// token of this machine; the origin only lets it ask.
pub fn sibling_page(headers: &HeaderMap) -> bool {
    let origin = headers.get(header::ORIGIN).and_then(|o| o.to_str().ok());
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok());
    let (Some(origin), Some(host)) = (origin, host) else { return false };
    siblings(origin, host)
}

pub fn siblings(origin: &str, host: &str) -> bool {
    let Some(origin) = origin.strip_prefix("https://") else { return false };
    let host = host.rsplit_once(':').filter(|(_, port)| port.bytes().all(|b| b.is_ascii_digit())).map_or(host, |(name, _)| name);
    let tailnet = |name: &str| -> Option<String> {
        let name = name.to_ascii_lowercase();
        let (machine, rest) = name.split_once('.')?;
        // `<machine>.<tailnet>.ts.net`, nothing deeper and nothing else.
        let ok = !machine.is_empty() && rest.ends_with(".ts.net") && rest.matches('.').count() == 2;
        ok.then(|| rest.to_owned())
    };
    matches!((tailnet(origin), tailnet(host)), (Some(a), Some(b)) if a == b)
}

fn local_name(host: &str) -> bool {
    let name = match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or_default(),
        None => host.rsplit_once(':').filter(|(_, port)| port.bytes().all(|b| b.is_ascii_digit())).map_or(host, |(name, _)| name),
    };
    let name = name.to_ascii_lowercase();
    name == "localhost"
        || name.ends_with(".ts.net")
        || name.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback() || tailnet_ip(ip))
}

/// The link as a QR code: dark on light, with the quiet border phones need to find it.
pub fn qr_svg(link: &str) -> Option<String> {
    let code = qrcode::QrCode::with_error_correction_level(link.as_bytes(), qrcode::EcLevel::M).ok()?;
    Some(
        code.render::<qrcode::render::svg::Color>()
            .min_dimensions(232, 232)
            .dark_color(qrcode::render::svg::Color("#1a1918"))
            .light_color(qrcode::render::svg::Color("#f4f1ec"))
            .quiet_zone(true)
            .build()
            // Goes inside a page, where the XML declaration has no place.
            .split_once("?>")
            .map_or_else(String::new, |(_, svg)| svg.to_owned()),
    )
    .filter(|svg| !svg.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_of_machines_on_the_same_tailnet_are_siblings() {
        assert!(siblings("https://pc.tail1.ts.net", "clanker.tail1.ts.net"));
        assert!(siblings("https://pc.tail1.ts.net", "clanker.tail1.ts.net:443"));
        assert!(!siblings("http://pc.tail1.ts.net", "clanker.tail1.ts.net"));
        assert!(!siblings("https://pc.other.ts.net", "clanker.tail1.ts.net"));
        assert!(!siblings("https://evil.example", "clanker.tail1.ts.net"));
        assert!(!siblings("https://a.b.tail1.ts.net", "clanker.tail1.ts.net"));
        assert!(!siblings("https://pc.tail1.ts.net", "192.168.1.9"));
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.append(axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(), v.parse().unwrap());
        }
        h
    }

    #[test]
    fn only_the_tailnet_and_this_machine_get_the_page() {
        let lo: IpAddr = "127.0.0.1".parse().unwrap();
        assert!(from_tailnet(lo, &headers(&[])));
        assert!(from_tailnet(lo, &headers(&[("x-forwarded-for", "100.101.5.7")])));
        assert!(from_tailnet(lo, &headers(&[("x-forwarded-for", "fd7a:115c:a1e0::1234")])));
        // A phone that claims an address first still ends with the one Tailscale added.
        assert!(!from_tailnet(lo, &headers(&[("x-forwarded-for", "100.101.5.7, 203.0.113.9")])));
        assert!(!from_tailnet(lo, &headers(&[("x-forwarded-for", "203.0.113.9")])));
        assert!(!from_tailnet(lo, &headers(&[("x-forwarded-for", "100.101.5.7"), ("tailscale-funnel-request", "?1")])));
        assert!(from_tailnet("100.70.1.2".parse().unwrap(), &headers(&[])));
        assert!(!from_tailnet("192.168.0.20".parse().unwrap(), &headers(&[])));
        assert!(!from_tailnet("192.168.0.20".parse().unwrap(), &headers(&[("x-forwarded-for", "100.101.5.7")])));
        assert!(!from_tailnet("100.128.0.1".parse().unwrap(), &headers(&[])));
    }

    #[test]
    fn a_phone_may_follow_and_answer_threads_only() {
        assert!(may(&Method::GET, "/api/threads"));
        assert!(may(&Method::GET, "/api/threads/abc/events"));
        assert!(may(&Method::GET, "/api/threads/abc/peek"));
        assert!(may(&Method::GET, "/api/threads/abc/changes"));
        assert!(may(&Method::GET, "/api/threads/abc/gitstate") && may(&Method::GET, "/api/threads/abc/branches"));
        assert!(may(&Method::POST, "/api/threads/abc/gitdraft") && may(&Method::POST, "/api/threads/abc/ship"));
        assert!(may(&Method::GET, "/api/github/diff"));
        assert!(may(&Method::GET, "/api/phone/files/tree") && may(&Method::GET, "/api/phone/files/raw") && may(&Method::GET, "/api/phone/files/search"));
        assert!(may(&Method::GET, "/api/problems") && may(&Method::GET, "/api/problems/12") && may(&Method::POST, "/api/problems/12/thread"));
        assert!(!may(&Method::GET, "/api/problems/sources") && !may(&Method::GET, "/api/problems/vakten") && may(&Method::PATCH, "/api/problems/12") && !may(&Method::PATCH, "/api/problems/sources"));
        assert!(!may(&Method::POST, "/api/problems/report") && !may(&Method::POST, "/api/problems/sources"));
        assert!(may(&Method::GET, "/api/stats") && may(&Method::GET, "/api/devices") && !may(&Method::DELETE, "/api/devices/abc"));
        assert!(may(&Method::GET, "/api/routines") && may(&Method::POST, "/api/routines/abc/run") && may(&Method::POST, "/api/routines/abc/enabled"));
        assert!(!may(&Method::PUT, "/api/routines/abc") && !may(&Method::DELETE, "/api/routines/abc") && !may(&Method::POST, "/api/routines"));
        assert!(!may(&Method::POST, "/api/threads/abc/switch"), "moving the checkout stays on the machine");
        assert!(may(&Method::POST, "/api/threads/abc/tidy"));
        assert!(may(&Method::GET, "/api/hosts") && may(&Method::GET, "/api/usage") && may(&Method::GET, "/api/projects"));
        assert!(may(&Method::POST, "/api/phone/threads"));
        assert!(may(&Method::GET, "/api/phone/engines") && may(&Method::POST, "/api/phone/warm"));
        assert!(may(&Method::POST, "/api/phone/bring") && !may(&Method::POST, "/api/folders/bring") && !may(&Method::POST, "/api/folders/clone"), "the phone names the project and machines, never the target path or a clone url");
        assert!(!may(&Method::GET, "/api/engines") && !may(&Method::POST, "/api/threads/warm"), "the desktop's catalog names the vault's keys, and its warm takes any setting");
        assert!(may(&Method::GET, "/api/host") && may(&Method::GET, "/api/host/about") && may(&Method::GET, "/api/machine/ready"));
        assert!(may(&Method::GET, "/api/project") && may(&Method::POST, "/api/project/focus/summaries"));
        assert!(may(&Method::GET, "/api/project/outputs") && may(&Method::GET, "/api/project/outputs/raw"));
        assert!(may(&Method::POST, "/api/phone/focus"));
        assert!(may(&Method::PUT, "/api/phone/notify") && may(&Method::POST, "/api/phone/notify/test"));
        assert!(may(&Method::POST, "/api/phone/looking"));
        assert!(may(&Method::PATCH, "/api/threads/abc") && may(&Method::POST, "/api/threads/move"));
        assert!(may(&Method::PUT, "/api/prefs/permissions") && may(&Method::POST, "/api/github/action"));
        assert!(!may(&Method::PUT, "/api/prefs/config"), "no hand edits of the config");
        assert!(!may(&Method::POST, "/api/hosts"), "adding machines stays on the PC");
        assert!(!may(&Method::POST, "/api/hosts/pc/terminals"), "no terminals");
        assert!(!may(&Method::POST, "/api/project/focus"), "only ticking, through /api/phone/focus");
        assert!(!may(&Method::GET, "/api/project/file") && !may(&Method::GET, "/api/project/raw") && !may(&Method::GET, "/api/project/tree"));
        assert!(!may(&Method::POST, "/api/threads"), "not the whole NewThread: no unattended runs or attempts");
        assert!(!may(&Method::POST, "/api/hosts") && !may(&Method::DELETE, "/api/projects"));
        assert!(!may(&Method::POST, "/api/threads/abc/gist"), "reading the card, not paying for a new one");
        assert!(may(&Method::POST, "/api/threads/abc/messages"));
        assert!(may(&Method::POST, "/api/threads/abc/permissions/7"));
        assert!(may(&Method::POST, "/api/threads/abc/cancel"));
        assert!(may(&Method::GET, "/api/stream"));
        assert!(may(&Method::POST, "/api/ideas"));
        assert!(!may(&Method::DELETE, "/api/ideas/abc"), "keeping one, not taking any back");
        assert!(!may(&Method::DELETE, "/api/threads/abc"));
        assert!(!may(&Method::POST, "/api/threads"));
        assert!(!may(&Method::GET, "/api/secrets"));
        assert!(!may(&Method::POST, "/api/pair"));
        assert!(!may(&Method::GET, "/api/threads/abc/files"));
        assert!(!may(&Method::POST, "/api/threads//messages"));
    }

    #[test]
    fn the_page_is_known_by_its_own_address() {
        let page = |origin: &str, host: &str| own_page(&headers(&[("origin", origin), ("host", host)]));
        assert!(page("https://pc.tail1234.ts.net", "pc.tail1234.ts.net"));
        assert!(page("http://127.0.0.1:7878", "127.0.0.1:7878"));
        assert!(page("http://localhost:7878", "localhost:7878"));
        assert!(!page("https://evil.example", "pc.tail1234.ts.net"));
        // DNS rebinding: the origin matches the host, but the name is not this machine's.
        assert!(!page("http://evil.example:7878", "evil.example:7878"));
        assert!(!page("http://192.168.0.5:7878", "192.168.0.5:7878"));
    }

    #[test]
    fn the_qr_code_is_an_svg() {
        let svg = qr_svg("https://pc.tail1234.ts.net/phone#pair=ABCD-EFGH").unwrap();
        assert!(svg.contains("<svg") && svg.contains("#1a1918"));
    }
}
