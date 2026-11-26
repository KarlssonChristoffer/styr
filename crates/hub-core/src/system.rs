//! Threads Hub starts on its own, and who owns them.
//!
//! Every thread an agent of Hub starts (Vakten, a routine run, the merge queue, a widget build,
//! a fix thread, Customs' monthly update, a thread another thread started through Hub's tools)
//! is started with its [`Owner`] (`Hub::create_thread_as`). The owner is what the app reads:
//! lists mark the thread with a robot and the owner's name, and a *system* owner's threads
//! (Vakten, routines, merge queue, widgets: Hub's own, not one project's) are folded under "From
//! Hub" in the inbox and come to its top only when they need the user. Errands (the rest) are done in a
//! project and listed there.
//!
//! How each kind is put away:
//! - a routine run settles once its report is in (the report is on the Routines page);
//! - an errand settles when its pull request merges, as any thread does (`auto_settle`);
//! - Vakten's chat is never settled, and no longer pinned to the top of the inbox.
//!
//! [`backfill`] gives threads from before owners existed theirs, once.

use anyhow::Result;

use crate::model::{Owner, OwnerKind, Routine, Thread};
use crate::store::Store;

/// Set once [`backfill`] has run, so it runs once per database.
const BACKFILLED: &str = "system.owners";

/// The owner of a thread that thread `by` starts through Hub's tools: that thread, by its title.
/// None when the user's own app asked (no calling thread).
pub fn agent(by: Option<&Thread>) -> Option<Owner> {
    by.map(|t| Owner::new(OwnerKind::Agent, t.title.clone(), Some(t.id.clone())))
}

/// A fix thread's owner: Vakten when it started it, else the Fix now button.
pub fn fixer(by: &str, problem: i64) -> Owner {
    let name = if by == "Vakten" { "Vakten" } else { "Fix now" };
    Owner::new(OwnerKind::Fixer, name, Some(problem.to_string()))
}

/// Gives threads started before owners existed theirs, from what Hub already knows about them:
/// Vakten's thread (`vakten`), routines' runs, problems' fix threads, and the titles the merge
/// queue, widget builds and Customs give theirs. Unpins Vakten's thread, which used to be pinned so
/// it stayed out of the way. Runs once (see [`backfilled`]); each changed thread goes to `publish`.
/// Hub calls it at start (`Hub::backfill_owners`) with what it read for it.
pub async fn backfill(store: &Store, vakten: Option<String>, routines: &[Routine], publish: impl Fn(Thread)) -> Result<()> {
    let mut owners: Vec<(String, Owner)> = Vec::new();
    if let Some(id) = vakten {
        owners.push((id, Owner::new(OwnerKind::Vakten, "Vakten", None)));
    }
    for r in routines {
        for run in &r.runs {
            if let Some(id) = &run.thread {
                owners.push((
                    id.clone(),
                    Owner::new(OwnerKind::Routine, r.name.clone(), Some(r.id.clone())),
                ));
            }
        }
    }
    let fixes = store
        .with(|c| {
            let mut q =
                c.prepare("SELECT id, thread_id FROM problems WHERE thread_id IS NOT NULL")?;
            let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
        .unwrap_or_default();
    let threads = store.threads().await?;
    for (problem, id) in fixes {
        // Only the ones Hub started: a thread the user tied to a problem stays theirs.
        if threads
            .iter()
            .any(|t| t.id == id && t.title.starts_with(&format!("Fix #{problem}")))
        {
            owners.push((id, fixer("Vakten", problem)));
        }
    }
    for t in &threads {
        let by_title = if t.title.starts_with("Widget · ") {
            Some(Owner::new(OwnerKind::Widget, "Widget", None))
        } else if t.title == "Monthly dependency update" {
            Some(Owner::new(OwnerKind::Customs, "Customs", None))
        } else {
            None
        };
        if let Some(owner) = by_title {
            owners.push((t.id.clone(), owner));
        }
    }
    for (id, owner) in owners {
        let Some(t) = threads.iter().find(|t| t.id == id) else {
            continue;
        };
        if t.owner.is_some() {
            continue;
        }
        let vakten = owner.kind == OwnerKind::Vakten;
        let thread = store
            .mark_thread(&id, move |t| {
                t.owner = Some(owner);
                if vakten {
                    t.pinned_at = None;
                }
            })
            .await?;
        publish(thread);
    }
    store.set_value(BACKFILLED, "1").await
}

/// Whether [`backfill`] has run on this database.
pub async fn backfilled(store: &Store) -> Result<bool> {
    Ok(store.get_value(BACKFILLED).await?.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_kinds() {
        for kind in [
            OwnerKind::Vakten,
            OwnerKind::Routine,
            OwnerKind::Merge,
            OwnerKind::Widget,
        ] {
            assert!(kind.system(), "{kind:?}");
        }
        for kind in [OwnerKind::Fixer, OwnerKind::Customs, OwnerKind::Agent] {
            assert!(!kind.system(), "{kind:?}");
        }
    }

    #[test]
    fn owner_reads_back() {
        let owner = fixer("Vakten", 12);
        let json = serde_json::to_string(&owner).unwrap();
        assert_eq!(json, r#"{"kind":"fixer","name":"Vakten","id":"12"}"#);
        assert_eq!(serde_json::from_str::<Owner>(&json).unwrap(), owner);
        assert_eq!(fixer("you", 3).name, "Fix now");
    }
}
