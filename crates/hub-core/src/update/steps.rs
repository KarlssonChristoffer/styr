//! Running the build steps in the updater's clone (`run_steps`, also used by the merge queue) into
//! a capped `update.log`: skipping an `npm install` whose package files match the stamp, the
//! steps' temp folder, and resetting the clone to its commit before a build.

use crate::locked::ResultExt;
use super::*;

/// Runs `steps` in the checked out source, into `update.log`, with the update's build settings.
/// Also used by the merge queue, which builds the update from the tested group before it merges.
pub(crate) fn run_steps(
    cfg: &UpdateConfig,
    data: &Path,
    src: &Path,
    target: &Path,
    steps: &[crate::config::BuildStep],
    progress: &dyn Fn(String),
    stop: &AtomicBool,
) -> Result<()> {
    let log_path = data.join("update.log");
    std::fs::create_dir_all(data)?;
    cap_log(&log_path);
    check_space(cfg, target)?;
    // No incremental caches and no debug info for the tests: a build folder that is rebuilt
    // for every version gains nothing from them but gigabytes.
    let env = vec![
        ("CARGO_TARGET_DIR", target.display().to_string()),
        // The tests export TypeScript bindings; into the build folder, not the clone.
        ("TS_RS_EXPORT_DIR", target.join("ts-bindings").display().to_string()),
        ("CARGO_INCREMENTAL", "0".to_owned()),
        ("CARGO_PROFILE_DEV_DEBUG", "0".to_owned()),
        // Half the cores: the build runs beside your work, and every test binary maps the whole
        // core library, which ran Windows out of page file with all cores at once.
        ("CARGO_BUILD_JOBS", build_jobs().to_string()),
    ];
    // Everything the steps put in the temp folder (thousands of test folders a build, some kept
    // open on Windows until the run ends) goes in one of the build's own, emptied after. It sits
    // in Hub's data folder, not the build folder: that is `src/target` by default, inside the
    // source clone, where the tests that need a folder outside any repository would find one.
    let temp = TempFolder::new(data.join("test-temp"));
    let mut env = env;
    for key in ["TEMP", "TMP", "TMPDIR"] {
        env.push((key, temp.0.display().to_string()));
    }
    // Pages left by an earlier build must not pass for this version's (`install` copies them).
    crate::locked::clean_dir(src.join("app/dist"));
    let limit = Duration::from_secs(cfg.step_timeout_mins.max(1) * 60);
    for (i, step) in steps.iter().enumerate() {
        let lock = npm_install_stamp(src, step);
        if lock.as_ref().is_some_and(|(dir, stamp)| node_modules_stamp(dir).as_deref() == Some(stamp.as_str())) {
            progress(format!("Step {} of {}: {} skipped, the packages are unchanged", i + 1, steps.len(), step.command.join(" ")));
            continue;
        }
        progress(format!("Step {} of {}: {}", i + 1, steps.len(), step.command.join(" ")));
        let mut env = env.clone();
        let (mut one_job, mut cleared) = (false, false);
        loop {
            wait_for_dictation(&|| progress(format!("Step {} of {} waits while you dictate: {}", i + 1, steps.len(), step.command.join(" "))), stop)?;
            wait_for_memory(&|gb| progress(format!("Step {} of {} waits for {gb} GB of free memory: {}", i + 1, steps.len(), step.command.join(" "))), stop)?;
            let Err(err) = run_logged(&src.join(&step.dir), &step.command, &env, &log_path, limit, stop) else {
                if let Some((dir, stamp)) = &lock {
                    std::fs::write(dir.join("node_modules/.hub-lock-stamp"), stamp).warn_on_err("write file");
                }
                break;
            };
            let text = format!("{err:#}");
            if stop.load(Ordering::Relaxed) {
                return Err(err);
            }
            // Stepped aside for dictation: the same step again once it is quiet, not a failure.
            if err.downcast_ref::<GaveWay>().is_some() {
                continue;
            }
            // Out of memory is the machine's state, not the commit's fault: once more, one job at a time.
            if out_of_memory(&text) && !one_job {
                one_job = true;
                progress(format!("Step {} of {} again, one job at a time: {}", i + 1, steps.len(), step.command.join(" ")));
                env.retain(|(k, _)| *k != "CARGO_BUILD_JOBS");
                env.push(("CARGO_BUILD_JOBS", "1".to_owned()));
            // A build that was killed or half deleted leaves crates cargo takes as built but rustc
            // cannot use. Not the commit's fault either: once more from an empty profile folder.
            } else if broken_build_folder(&text) && !cleared {
                cleared = true;
                let profile = if step.command.iter().any(|a| a == "--release") { "release" } else { "debug" };
                progress(format!("Step {} of {} again, after clearing the broken {profile} build: {}", i + 1, steps.len(), step.command.join(" ")));
                crate::locked::clean_dir(target.join(profile));
            } else {
                return Err(err);
            }
        }
    }
    Ok(())
}

/// For an `npm install` step: its folder and a stamp of `package.json` + `package-lock.json`.
/// When `node_modules` carries the same stamp, the install has nothing to do and is skipped.
pub(super) fn npm_install_stamp(src: &Path, step: &crate::config::BuildStep) -> Option<(PathBuf, String)> {
    use sha2::{Digest, Sha256};
    if !(step.command.get(1).map(String::as_str) == Some("install") && step.command[0].starts_with("npm")) {
        return None;
    }
    let dir = src.join(&step.dir);
    let lock = std::fs::read(dir.join("package-lock.json")).ok()?;
    let pkg = std::fs::read(dir.join("package.json")).ok()?;
    let mut h = Sha256::new();
    h.update(&pkg);
    h.update([0]);
    h.update(&lock);
    Some((dir, format!("{:x}", h.finalize())))
}

/// The stamp the last successful install left in `node_modules`.
pub(super) fn node_modules_stamp(dir: &Path) -> Option<String> {
    std::fs::read_to_string(dir.join("node_modules/.hub-lock-stamp")).ok()
}

/// The steps' temp folder: emptied when made (what a killed run left) and when the run ends.
pub(super) struct TempFolder(PathBuf);

impl TempFolder {
    fn new(path: PathBuf) -> Self {
        crate::locked::clean_dir(&path);
        std::fs::create_dir_all(&path).warn_on_err("create the temp folder");
        Self(path)
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        crate::locked::clean_dir(&self.0);
    }
}

/// Makes the source folder match its last commit again. The default folder belongs to the
/// updater: what builds leave behind there (a rewritten lockfile, generated bindings) is
/// discarded, while ignored folders such as `target` and `node_modules` stay for the next build.
/// A folder set in the config may be someone's own checkout, so it is left alone when changed.
pub(super) fn clean(cfg: &UpdateConfig, src: &Path) -> Result<()> {
    let status = git(src, &["status", "--porcelain"])?;
    if status.is_empty() {
        return Ok(());
    }
    if cfg.source_dir.is_some() {
        return Err(DirtySource { path: src.display().to_string() }.into());
    }
    tracing::info!("discarding leftovers in {}:\n{status}", src.display());
    git(src, &["reset", "--quiet", "--hard"])?;
    git(src, &["clean", "--quiet", "-fd"])?;
    Ok(())
}

/// The configured source folder has changes of its own. Not the commit's fault either.
#[derive(Debug, thiserror::Error)]
#[error("the update folder {path} has local changes; not touching it")]
pub struct DirtySource {
    pub path: String,
}

pub use crate::disk::{LowDisk, check_space, target_dir, trim};

/// `update.log` is appended to by every build (a few hundred KB each). Past `LOG_CAP` it is cut
/// to its last `LOG_KEEP` bytes, so it never grows without end on a small drive.
pub(crate) fn cap_log(log_path: &Path) {
    const LOG_CAP: u64 = 8 << 20;
    const LOG_KEEP: usize = 2 << 20;
    let Ok(meta) = std::fs::metadata(log_path) else { return };
    if meta.len() <= LOG_CAP {
        return;
    }
    let Ok(text) = std::fs::read(log_path) else { return };
    let tail = &text[text.len().saturating_sub(LOG_KEEP)..];
    // From the next whole line, so the file does not start mid-line.
    let from = tail.iter().position(|b| *b == b'\n').map_or(0, |i| i + 1);
    let mut kept = format!("== earlier output cut at {} MB\n", LOG_CAP >> 20).into_bytes();
    kept.extend_from_slice(&tail[from..]);
    std::fs::write(log_path, kept).warn_on_err("write file");
}
