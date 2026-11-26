//! Hands this machine's installed Hub to another machine of the same kind, so a new Windows PC
//! installs in a minute instead of compiling Hub for 10-25 minutes (`scripts/install.ps1 -From`).
//! The package is the current `bin/<version>` folder as a `.tar.gz`, made with the system `tar`
//! (Windows 10 and Mac have one) once per version into `<data dir>/dist/` and sent from there.
//! Only the same operating system and CPU can run it, so a request for another kind is refused
//! with what this machine is. The call needs a connection token like the rest of the API; a new
//! machine gets one by redeeming a pairing code (`pairing.rs`) first.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// The folder the packages are kept in, inside the data folder.
const DIR: &str = "dist";

/// One package is made at a time, so a retry or a second PC never writes into the same file.
static MAKING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// What this build runs on, as the install script names it: `windows-x86_64`, `macos-aarch64`.
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// A package of the installed version for `wanted`, made if it is not there yet: its path and
/// version. Errors say why there is none (another kind of machine, no installed version).
pub async fn package(data: &Path, wanted: &str) -> Result<(PathBuf, String)> {
    let mine = platform();
    if wanted != mine {
        bail!("this Hub runs on {mine} and has no build for {wanted}");
    }
    let version = hub_install::current_version(data).context("this Hub is not an installed version, so it has nothing to hand on")?;
    let from = hub_install::bin_dir(data).join(&version);
    if !from.join(hub_install::exe("hubd")).is_file() {
        bail!("{version} is not complete here");
    }
    let dir = data.join(DIR);
    tokio::fs::create_dir_all(&dir).await?;
    let file = dir.join(format!("{version}.{mine}.tar.gz"));
    let _making = MAKING.lock().await;
    if !file.is_file() {
        let partial = dir.join(format!("{version}.{mine}.partial"));
        let out = crate::proc::tokio_command("tar")
            .arg("-czf")
            .arg(&partial)
            .arg("-C")
            .arg(&from)
            .arg(".")
            .output()
            .await
            .context("running tar")?;
        if !out.status.success() {
            crate::locked::clean_file_async(&partial).await;
            bail!("tar: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        tokio::fs::rename(&partial, &file).await?;
        // Older versions' packages are of no use to anyone.
        if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path != file && !path.extension().is_some_and(|e| e == "partial") {
                    crate::locked::clean_file_async(path).await;
                }
            }
        }
    }
    Ok((file, version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn refuses_another_kind_and_a_missing_version() {
        let data = tempfile::tempdir().unwrap();
        let other = if platform() == "windows-x86_64" { "macos-aarch64" } else { "windows-x86_64" };
        assert!(package(data.path(), other).await.unwrap_err().to_string().contains("no build for"));
        assert!(package(data.path(), &platform()).await.unwrap_err().to_string().contains("not an installed version"));
    }

    #[tokio::test]
    async fn packs_the_current_version_once() {
        let data = tempfile::tempdir().unwrap();
        let bin = hub_install::bin_dir(data.path()).join("abc123");
        std::fs::create_dir_all(bin.join("ui")).unwrap();
        std::fs::write(bin.join(hub_install::exe("hubd")), "x").unwrap();
        std::fs::write(bin.join("ui").join("index.html"), "<p>").unwrap();
        std::fs::write(bin.join("VERSION"), "abc123").unwrap();
        hub_install::set_current(data.path(), "abc123").unwrap();
        let (file, version) = package(data.path(), &platform()).await.unwrap();
        assert_eq!(version, "abc123");
        let listing = std::process::Command::new("tar").arg("-tzf").arg(&file).output().unwrap();
        let listing = String::from_utf8_lossy(&listing.stdout);
        assert!(listing.contains("VERSION") && listing.contains("index.html") && listing.contains("hubd"), "{listing}");
        let again = package(data.path(), &platform()).await.unwrap();
        assert_eq!(again.0, file);
    }
}
