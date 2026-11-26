//! The `Hub` half of `triage.rs`: reading and saving the thread and chat-detail settings, and the
//! loop that looks up each thread's pull request and settles what is done. The types and
//! `parse_time` stay in `triage.rs`, which then names nothing above it.

use crate::locked::LockExt;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use serde::Deserialize;

use super::Hub;
use crate::model::{StreamMsg, Thread, ThreadId, ThreadStatus, now_ms};
use crate::triage::{ChatDetail, ThreadPr, ThreadPrefs, parse_time};

const PREFS: &str = "threads.prefs";
const TICK: Duration = Duration::from_secs(300);
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const DETAIL: &str = "chat.detail";

impl Hub {
    pub async fn thread_prefs(&self) -> Result<ThreadPrefs> {
        Ok(match self.store.get_value(PREFS).await? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => ThreadPrefs::default(),
        })
    }

    pub async fn set_thread_prefs(&self, prefs: ThreadPrefs) -> Result<ThreadPrefs> {
        if prefs.settle_after_days > 365 {
            bail!("settle after at most a year");
        }
        self.store.set_value(PREFS, &serde_json::to_string(&prefs)?).await?;
        Ok(prefs)
    }

    pub async fn chat_detail(&self) -> Result<ChatDetail> {
        Ok(match self.store.get_value(DETAIL).await? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => ChatDetail::default(),
        })
    }

    pub async fn set_chat_detail(&self, detail: ChatDetail) -> Result<ChatDetail> {
        const LEVELS: [&str; 4] = ["minimal", "quiet", "normal", "all"];
        let ok = |l: &String| l.is_empty() || LEVELS.contains(&l.as_str());
        if !ok(&detail.default) || !detail.threads.values().all(|l| LEVELS.contains(&l.as_str())) {
            bail!("a level is minimal, quiet, normal or all");
        }
        self.store.set_value(DETAIL, &serde_json::to_string(&detail)?).await?;
        Ok(detail)
    }

    pub fn thread_prs(&self) -> BTreeMap<ThreadId, ThreadPr> {
        self.prs.0.locked().clone()
    }

    /// Looks up pull requests and settles what is done, every few minutes.
    pub(crate) async fn tidy_threads(self: Arc<Self>) {
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            if let Err(err) = self.tidy_once().await {
                tracing::warn!("tidying threads: {err:#}");
            }
            tokio::time::sleep(TICK).await;
        }
    }

    pub async fn tidy_once(self: &Arc<Self>) -> Result<()> {
        let prefs = self.thread_prefs().await?;
        let threads: Vec<Thread> = self.store.threads().await?.into_iter().filter(|t| !t.archived).collect();
        let merged = self.look_up_prs(&threads).await;
        if prefs.watch_prs
            && let Err(err) = self.auto_watch(&threads).await
        {
            tracing::warn!("watching pull requests by itself: {err:#}");
        }
        // A thread that watches its pull request is still working on it.
        let watched = self.watches().await.unwrap_or_default();
        let now = now_ms();
        for t in threads {
            // Only threads that are done for now, that the user has not kept out of it.
            if t.settled_at.is_some() || !t.auto_settle || t.pinned_at.is_some() || t.status != ThreadStatus::Idle || watched.contains_key(&t.id) {
                continue;
            }
            if t.snoozed_until.is_some_and(|until| until > now) {
                continue;
            }
            // An empty thread counts from when it was made.
            let last = t.returned_at.max(t.sent_at).unwrap_or(t.created_at);
            // A thread read as finished (`reading.rs`) that you have seen since settles after a day.
            let finished = t.reading.as_deref() == Some("done") && t.seen_at.is_some_and(|seen| seen >= last);
            let days = if finished { prefs.settle_after_days.min(1) } else { prefs.settle_after_days };
            let idle = days > 0 && now - last > days as i64 * DAY_MS;
            // A merge before the user's latest message was before they took the work up again.
            let done = prefs.settle_on_merge && merged.get(&t.id).is_some_and(|at| t.sent_at.is_none_or(|sent| *at > sent));
            if idle || done {
                let marked = self.store.mark_thread(&t.id, move |t| t.settled_at = Some(now)).await?;
                tracing::info!(thread = %t.id, "settled by itself ({})", if done { "pull request merged" } else { "idle" });
                self.publish(StreamMsg::Thread { thread: marked });
            }
        }
        Ok(())
    }

    /// Finds the pull request of each thread's branch, one `gh` call per repository. Returns
    /// when each merged one merged, by thread.
    async fn look_up_prs(&self, threads: &[Thread]) -> HashMap<ThreadId, i64> {
        let mut by_repo: HashMap<PathBuf, Vec<(ThreadId, String)>> = HashMap::new();
        let places: Vec<(ThreadId, String)> = threads.iter().filter(|t| t.host.is_none()).map(|t| (t.id.clone(), t.cwd.clone())).collect();
        let found = tokio::task::spawn_blocking(move || {
            places
                .into_iter()
                .filter_map(|(id, cwd)| {
                    let cwd = Path::new(&cwd);
                    let branch = crate::git::branch_of(cwd)?;
                    let root = repo_root(cwd)?;
                    Some((root, id, branch))
                })
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        for (root, id, branch) in found {
            if !matches!(branch.as_str(), "main" | "master" | "trunk" | "develop") {
                by_repo.entry(root).or_default().push((id, branch));
            }
        }
        let mut prs = BTreeMap::new();
        let mut merged = HashMap::new();
        for (root, wanted) in by_repo {
            let args = ["pr", "list", "--state", "all", "--limit", "100", "--json", "number,title,url,state,isDraft,headRefName,mergedAt"];
            let Ok(out) = crate::proc::gh_within(&root, &args, Duration::from_secs(10)).await else { continue };
            let Ok(list) = serde_json::from_str::<Vec<GhPr>>(&out) else { continue };
            for (id, branch) in wanted {
                // The newest pull request of the branch; gh lists newest first.
                let Some(pr) = list.iter().find(|p| p.head_ref_name == branch) else { continue };
                let state = match (pr.state.as_str(), pr.is_draft) {
                    ("MERGED", _) => "merged",
                    ("CLOSED", _) => "closed",
                    (_, true) => "draft",
                    _ => "open",
                };
                if let Some(at) = pr.merged_at.as_deref().and_then(parse_time) {
                    merged.insert(id.clone(), at);
                }
                prs.insert(id, ThreadPr { number: pr.number, title: pr.title.clone(), url: pr.url.clone(), state: state.into() });
            }
        }
        *self.prs.0.locked() = prs;
        merged
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPr {
    number: u32,
    title: String,
    url: String,
    state: String,
    #[serde(default)]
    is_draft: bool,
    head_ref_name: String,
    merged_at: Option<String>,
}

/// The folder holding `.git` (or the `.git` file of a worktree) for `cwd`.
fn repo_root(cwd: &Path) -> Option<PathBuf> {
    cwd.ancestors().find(|d| d.join(".git").exists()).map(Path::to_path_buf)
}
