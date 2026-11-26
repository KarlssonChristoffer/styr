//! Git bundles of this machine's clone, so another machine gets code without GitHub: a version
//! (or only what the other machine lacks of it) for a host's updater (`ship`) and for the tests
//! run on another machine (`update::checks`), and the branch for an install (`ship`).

use anyhow::{Context, Result, bail};

use crate::locked::ResultExt;

/// The commit `version` in this machine's clone as a git bundle, and the name of the one ref in
/// it. A ref of its own for the bundle, so the clone's branches stay as they are.
pub(super) async fn version_bundle(source: &std::path::Path, version: &str) -> Result<(Vec<u8>, String)> {
    version_bundle_since(source, version, None).await
}

/// `version_bundle`, but only what `since` (a commit the receiver has) does not already have.
/// The whole history when `since` is not in this clone.
pub(crate) async fn version_bundle_since(source: &std::path::Path, version: &str, since: Option<&str>) -> Result<(Vec<u8>, String)> {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let name = format!("refs/hub-ship/{id}");
    let file = std::env::temp_dir().join(format!("hub-{id}.bundle"));
    // Under git.rs's long limit: a bundle of the whole history of a big clone takes a while.
    let git = |args: Vec<String>| async move {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = crate::git::output_async(source, &args, crate::git::LONG).await?;
        if !out.status.success() {
            bail!("git {}: {}", args[0], String::from_utf8_lossy(&out.stderr).trim());
        }
        anyhow::Ok(())
    };
    git(vec!["update-ref".into(), name.clone(), format!("{version}^{{commit}}")])
        .await
        .with_context(|| format!("{version} is not in {}", source.display()))?;
    let known = match since {
        Some(since) => git(vec!["cat-file".into(), "-e".into(), format!("{since}^{{commit}}")]).await.is_ok().then(|| format!("^{since}")),
        None => None,
    };
    let mut args: Vec<String> = vec!["bundle".into(), "create".into(), "--quiet".into(), file.display().to_string(), name.clone()];
    args.extend(known);
    let made = git(args).await;
    git(vec!["update-ref".into(), "-d".into(), name.clone()]).await.warn_on_err("delete the bundle's ref");
    made?;
    let bytes = tokio::fs::read(&file).await;
    crate::locked::clean_file_async(&file).await;
    Ok((bytes?, name))
}

/// This machine's copy of the repository as a git bundle of `branch`, so a host can be installed
/// without logging in to GitHub there. None when there is no copy.
pub(super) async fn bundle(source: &std::path::Path, branch: &str) -> Option<Vec<u8>> {
    if !source.join(".git").exists() {
        return None;
    }
    let file = std::env::temp_dir().join(format!("hub-{}.bundle", uuid::Uuid::new_v4().simple()));
    let (file_arg, branch_ref) = (file.display().to_string(), format!("refs/remotes/origin/{branch}"));
    let out = crate::git::output_async(source, &["bundle", "create", &file_arg, &branch_ref], crate::git::LONG).await.ok()?;
    let bytes = if out.status.success() { tokio::fs::read(&file).await.ok() } else { None };
    crate::locked::clean_file_async(&file).await;
    bytes
}
