//! Who may call the API. Browsers are told apart by their `Origin` header, which a web page
//! cannot forge: only the app's own origins get in. Everything else (another program on this
//! machine, a core on another host, a phone) shows the connection token, as VS Code's server and
//! Jupyter do. The token lives in `<data dir>/token`, readable only by the user Hub runs as.
//! The door that checks each request is `guard.rs`; this file keeps the token and the plain values
//! and helpers around it.

use std::path::Path;

use anyhow::{Context, Result};
use axum::http::header;

/// The query parameter a WebSocket carries the token in, since browsers cannot set its headers.
pub const TOKEN_PARAM: &str = "token";

/// Where threads call their API connections through Hub's gateway (`gateway::forward`).
pub const GATEWAY: &str = "/gw/";
/// Where a service that logs you in sends the browser back (`gateway::callback`).
pub const OAUTH_CALLBACK: &str = "/api/oauth/callback";

/// Put on a request from a phone's token, or forwarded by another Hub for one (`NARROW_HEADER`):
/// such a caller may make Hub's safety stricter but never looser (`loosens`).
#[derive(Debug, Clone, Copy)]
pub struct Narrow;

/// The header a Hub adds when it forwards a narrow caller's request to another machine
/// (`remote::Host::forward`), so that machine holds it to the same limits. Only a caller with a
/// full token can forward, and the header only ever takes rights away.
pub const NARROW_HEADER: &str = "x-hub-narrow";

/// Where agents outside Hub reach Hub's tools with an agent key (`hub_tools.rs`).
pub const AGENTS: &str = "/mcp";

/// The token in `<data dir>/token`, created on first use.
pub fn token(data_dir: &Path) -> Result<String> {
    let path = data_dir.join("token");
    if let Ok(token) = std::fs::read_to_string(&path)
        && !token.trim().is_empty()
    {
        return Ok(token.trim().to_owned());
    }
    std::fs::create_dir_all(data_dir)?;
    let token = fresh();
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    std::io::Write::write_all(&mut options.open(&path)?, token.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(token)
}

pub(crate) fn fresh() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

/// The token a thread's engine shows at `/mcp/{thread}` (`tools_endpoint`): a hash of the
/// connection token and the thread id, so it opens Hub's tools for that thread and nothing else.
/// An agent that is talked into calling Hub's API with what it was given gets `401`, and the
/// connection token itself never reaches an engine. Giving the hash to the engine is safe
/// because SHA-256 cannot be run backwards, and a thread id cannot be chosen to collide.
pub fn tool_token(token: &str, thread: &str) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"hub tools\0");
    hasher.update(token.as_bytes());
    hasher.update(b"\0");
    hasher.update(thread.as_bytes());
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// The URL a thread's engine reaches Hub's tools at, and the token it has to show: one for this
/// thread's tools only (`tool_token`), never the connection token, so an agent talked into
/// calling Hub's API with it gets nowhere. A core bound to every interface (0.0.0.0 or ::) is
/// reached over loopback, since that is no address to dial.
pub fn tools_endpoint(config: &crate::Config, thread_id: &str) -> (String, anyhow::Result<String>) {
    let token = token(&config.data_dir).map(|t| tool_token(&t, thread_id));
    (format!("http://{}/mcp/{thread_id}", local_addr(config)), token)
}

/// `url` with the tool token in its query, for an engine that does not show headers
/// (`EngineTraits::mcp_token_in_url`). `guard::check` reads the same `token` parameter it does for
/// the phone, and still only lets this token into this thread's tools.
pub fn with_token(url: &str, token: &str) -> String {
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}{}={token}", TOKEN_PARAM)
}

/// Where a program on this machine reaches the core.
pub fn local_addr(config: &crate::Config) -> std::net::SocketAddr {
    let mut addr = config.bind;
    if addr.ip().is_unspecified() {
        addr.set_ip(match addr {
            std::net::SocketAddr::V4(_) => std::net::Ipv4Addr::LOCALHOST.into(),
            std::net::SocketAddr::V6(_) => std::net::Ipv6Addr::LOCALHOST.into(),
        });
    }
    addr
}

/// The bearer token a request shows, if any.
pub fn bearer(headers: &axum::http::HeaderMap) -> Option<&str> {
    headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).map(str::trim)
}

/// Whether `given` is this core's connection token.
pub fn is_token(config: &crate::Config, given: &str) -> bool {
    token(&config.data_dir).is_ok_and(|t| same(given.trim(), &t))
}

/// Compares without stopping at the first difference, so timing says nothing about the token.
pub(crate) fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Whether `new` lets agents do anything `old` did not: a higher level, a new always-allowed rule,
/// or a refused rule or protected path taken away. What a narrow caller may not do.
pub fn loosens(old: &crate::model::PermissionPrefs, new: &crate::model::PermissionPrefs) -> bool {
    use crate::model::PermissionLevel::*;
    let rank = |l: crate::model::PermissionLevel| match l {
        Ask => 0,
        Read => 1,
        Edit => 2,
        Everything => 3,
    };
    // Blank rules are dropped when saved (`Hub::set_permission_prefs`), so they count for nothing.
    let missing = |from: &[String], to: &[String]| from.iter().any(|r| !r.trim().is_empty() && !to.iter().any(|t| t.trim() == r.trim()));
    rank(new.level) > rank(old.level) || missing(&new.allow, &old.allow) || missing(&old.deny, &new.deny) || missing(&old.protected, &new.protected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stricter_safety_is_not_looser() {
        use crate::model::{PermissionLevel, PermissionPrefs};
        let old = PermissionPrefs { level: PermissionLevel::Edit, allow: vec!["Bash(npm test:*)".into()], deny: vec!["rm *".into()], protected: vec![".env".into()] };
        let with = |f: &dyn Fn(&mut PermissionPrefs)| {
            let mut p = old.clone();
            f(&mut p);
            p
        };
        assert!(!loosens(&old, &old));
        assert!(!loosens(&old, &with(&|p| p.level = PermissionLevel::Ask)));
        assert!(!loosens(&old, &with(&|p| p.allow.clear())));
        assert!(!loosens(&old, &with(&|p| p.deny.push("curl *".into()))));
        assert!(!loosens(&old, &with(&|p| p.allow.push("  ".into()))));
        assert!(loosens(&old, &with(&|p| p.level = PermissionLevel::Everything)));
        assert!(loosens(&old, &with(&|p| p.allow.push("*".into()))));
        assert!(loosens(&old, &with(&|p| p.deny.clear())));
        assert!(loosens(&old, &with(&|p| p.protected.clear())));
    }

    #[test]
    fn the_token_is_kept_between_starts() {
        let data = tempfile::tempdir().unwrap();
        let first = token(data.path()).unwrap();
        assert_eq!(first.len(), 64);
        assert_eq!(token(data.path()).unwrap(), first);
    }
}
