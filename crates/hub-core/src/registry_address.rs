//! Where Customs' registry (`customs_registry.rs`) is served and how an engine's package managers
//! are pointed at it: the plain values the auth layer, the engine start and the settings page need
//! without reaching the registry itself.

use crate::config::Config;

/// Where the registry is served; open without a token, like the registries it stands for.
pub const PREFIX: &str = "/registry/";

/// The variables that point an engine's package managers here. Cargo cannot be pointed by
/// environment alone; its source replacement goes in a config file (see `cargo_config`).
pub fn env(base: &str) -> Vec<(&'static str, String)> {
    let base = base.trim_end_matches('/');
    let npm = format!("{base}/registry/npm/");
    let pypi = format!("{base}/registry/pypi/simple/");
    vec![
        ("npm_config_registry", npm.clone()),
        ("YARN_NPM_REGISTRY_SERVER", npm.clone()),
        ("YARN_REGISTRY", npm),
        ("PIP_INDEX_URL", pypi.clone()),
        ("UV_INDEX_URL", pypi.clone()),
        ("UV_DEFAULT_INDEX", pypi),
    ]
}

/// What to put in `~/.cargo/config.toml` to send cargo through Customs too.
pub fn cargo_config(base: &str) -> String {
    format!(
        "[source.crates-io]\nreplace-with = \"hub-customs\"\n\n[source.hub-customs]\nregistry = \"sparse+{}/registry/crates/\"\n",
        base.trim_end_matches('/')
    )
}

/// The address engines on this machine reach the registry at.
pub fn base(config: &Config) -> String {
    config.customs_url.clone().unwrap_or_else(|| format!("http://{}", config.bind)).trim_end_matches('/').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_package_managers_here() {
        let env = env("http://127.0.0.1:7878/");
        assert!(env.contains(&("npm_config_registry", "http://127.0.0.1:7878/registry/npm/".into())));
        assert!(env.contains(&("PIP_INDEX_URL", "http://127.0.0.1:7878/registry/pypi/simple/".into())));
        assert!(cargo_config("http://h:1").contains("registry = \"sparse+http://h:1/registry/crates/\""));
    }
}
