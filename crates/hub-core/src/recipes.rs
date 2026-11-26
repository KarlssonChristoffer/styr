//! Recipes: saved jobs the user starts by hand on any project, like "update the dependencies" or
//! "fix the failing tests". Routines (`routines.rs`) run on a schedule and only report; a recipe
//! runs when the user (or an agent they asked) starts it, and does the work. A run is an ordinary
//! thread in the picked folder, marked `recipe: { id, name }`, whose first message is Hub's note
//! (do the whole job, stop at an open pull request), the recipe's prompt and the note for this run.
//!
//! Agents can hand Hub a recipe with the `suggest_recipe` tool (`hub_tools.rs`). A suggestion is
//! kept in the same list with `suggested` set, and is only offered to run once the user keeps it.
//!
//! Recipes are kept as one list under `recipes`, each with its last few runs. How a run went is
//! its thread's status, so nothing here follows the run.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::ctx::Ctx;
use crate::model::{Recipe, RecipeDraft, RecipeRun, RecipeSuggestion, RecipeTemplate, Recipes, now_ms};

const KEY: &str = "recipes";
/// Runs kept per recipe.
const KEEP_RUNS: usize = 20;
/// Suggestions waiting at once; the oldest goes when an agent suggests one more.
const KEEP_SUGGESTED: usize = 20;

/// One writer at a time for the list.
static LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

async fn load(ctx: &Ctx<'_>) -> Result<Vec<Recipe>> {
    Ok(match ctx.store.get_value(KEY).await? {
        Some(json) => serde_json::from_str(&json).context("reading the recipes")?,
        None => Vec::new(),
    })
}

async fn save(ctx: &Ctx<'_>, recipes: &[Recipe]) -> Result<()> {
    ctx.store.set_value(KEY, &serde_json::to_string(recipes)?).await
}

/// Changes the recipe `id` under the lock and returns it.
async fn change(ctx: &Ctx<'_>, id: &str, f: impl FnOnce(&mut Recipe) -> Result<()>) -> Result<Recipe> {
    let _guard = LOCK.lock().await;
    let mut all = load(ctx).await?;
    let recipe = all.iter_mut().find(|r| r.id == id).context("no such recipe")?;
    f(recipe)?;
    let out = recipe.clone();
    save(ctx, &all).await?;
    Ok(out)
}

/// Every recipe, suggestions included, and the starter recipes.
pub async fn list(ctx: &Ctx<'_>) -> Result<Recipes> {
    Ok(Recipes { recipes: load(ctx).await?, templates: templates() })
}

pub async fn create(ctx: &Ctx<'_>, draft: RecipeDraft) -> Result<Recipe> {
    let draft = checked(ctx, draft)?;
    let recipe = made(draft, None);
    let _guard = LOCK.lock().await;
    let mut all = load(ctx).await?;
    if let Some(same) = all.iter().find(|r| r.suggested.is_none() && same_name(&r.name, &recipe.name)) {
        bail!("there is a recipe called {:?} already", same.name);
    }
    all.push(recipe.clone());
    save(ctx, &all).await?;
    Ok(recipe)
}

fn made(draft: RecipeDraft, suggested: Option<RecipeSuggestion>) -> Recipe {
    Recipe {
        id: uuid::Uuid::now_v7().to_string(),
        name: draft.name,
        about: draft.about.unwrap_or_default(),
        prompt: draft.prompt,
        engine: draft.engine,
        folder: draft.folder,
        template: draft.template,
        suggested,
        created_at: now_ms(),
        runs: Vec::new(),
    }
}

/// Changes a recipe. Saving a suggestion keeps it.
pub async fn update(ctx: &Ctx<'_>, id: &str, draft: RecipeDraft) -> Result<Recipe> {
    let draft = checked(ctx, draft)?;
    change(ctx, id, |r| {
        r.name = draft.name;
        r.about = draft.about.unwrap_or_default();
        r.prompt = draft.prompt;
        r.engine = draft.engine;
        r.folder = draft.folder;
        r.suggested = None;
        Ok(())
    })
    .await
}

/// Keeps an agent's suggestion as it is.
pub async fn keep(ctx: &Ctx<'_>, id: &str) -> Result<Recipe> {
    change(ctx, id, |r| {
        r.suggested = None;
        Ok(())
    })
    .await
}

/// Deletes a recipe, or dismisses a suggestion.
pub async fn delete(ctx: &Ctx<'_>, id: &str) -> Result<()> {
    let _guard = LOCK.lock().await;
    let mut all = load(ctx).await?;
    all.retain(|r| r.id != id);
    save(ctx, &all).await
}

fn same_name(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn checked(ctx: &Ctx<'_>, mut draft: RecipeDraft) -> Result<RecipeDraft> {
    draft.name = draft.name.trim().chars().take(80).collect();
    if draft.name.is_empty() {
        bail!("give the recipe a name");
    }
    if draft.prompt.trim().is_empty() {
        bail!("say what the recipe should do");
    }
    draft.about = draft.about.map(|a| a.trim().chars().take(200).collect::<String>()).filter(|a| !a.is_empty());
    draft.folder = draft.folder.map(|f| f.trim().to_owned()).filter(|f| !f.is_empty());
    if let Some(folder) = &draft.folder
        && !std::path::Path::new(folder).is_dir()
    {
        bail!("folder {folder:?} does not exist on this machine");
    }
    draft.engine = draft.engine.filter(|e| !e.trim().is_empty());
    if let Some(engine) = &draft.engine
        && !ctx.config.engines.contains_key(engine)
    {
        bail!("no engine {engine:?} here");
    }
    Ok(draft)
}

/// An agent hands Hub a recipe. It waits as a suggestion until the user keeps it; suggesting one
/// with the name of a waiting suggestion replaces that one.
pub async fn suggest(ctx: &Ctx<'_>, draft: RecipeDraft, by: RecipeSuggestion) -> Result<Recipe> {
    // The agent's folder is a hint, not a rule: a recipe is for any project unless the user ties it.
    let draft = checked(ctx, RecipeDraft { folder: None, template: None, ..draft })?;
    let _guard = LOCK.lock().await;
    let mut all = load(ctx).await?;
    if let Some(kept) = all.iter().find(|r| r.suggested.is_none() && same_name(&r.name, &draft.name)) {
        bail!("the user already has a recipe called {:?}; run it with run_recipe, or suggest one under another name", kept.name);
    }
    all.retain(|r| !(r.suggested.is_some() && same_name(&r.name, &draft.name)));
    let recipe = made(draft, Some(by));
    all.push(recipe.clone());
    let waiting: Vec<String> = all.iter().filter(|r| r.suggested.is_some()).map(|r| r.id.clone()).collect();
    if waiting.len() > KEEP_SUGGESTED {
        let gone = &waiting[..waiting.len() - KEEP_SUGGESTED];
        all.retain(|r| !gone.contains(&r.id));
    }
    save(ctx, &all).await?;
    Ok(recipe)
}

/// The recipe called `name` (or with that id) that the user kept.
pub async fn find(ctx: &Ctx<'_>, name: &str) -> Result<Recipe> {
    let all = load(ctx).await?;
    all.iter()
        .filter(|r| r.suggested.is_none())
        .find(|r| r.id == name || same_name(&r.name, name))
        .cloned()
        .with_context(|| {
            let names: Vec<&str> = all.iter().filter(|r| r.suggested.is_none()).map(|r| r.name.as_str()).collect();
            format!("no recipe called {name:?}; the user has: {}", if names.is_empty() { "none".into() } else { names.join(", ") })
        })
}

/// How to start a run besides where.
#[derive(Default)]
pub struct Start {
    /// Anything to add this time.
    pub note: Option<String>,
    /// Session settings for the new thread.
    pub settings: BTreeMap<String, Value>,
    /// Who started it when it was not the user: the agent's mark (`from`).
    pub by: Option<Value>,
}

/// Recipe `id` and the folder a run of it goes to (its own folder wins), or why it cannot start.
/// `Hub::start_recipe` (`hub/recipes.rs`) starts it.
pub async fn to_start(ctx: &Ctx<'_>, id: &str, folder: Option<&str>) -> Result<(Recipe, String)> {
    let recipe = load(ctx).await?.into_iter().find(|r| r.id == id).context("no such recipe")?;
    if recipe.suggested.is_some() {
        bail!("keep the suggested recipe first");
    }
    let folder = recipe
        .folder
        .clone()
        .or_else(|| folder.map(str::trim).filter(|f| !f.is_empty()).map(str::to_owned))
        .context("pick the project to run it on")?;
    if !std::path::Path::new(&folder).is_dir() {
        bail!("folder {folder:?} does not exist on this machine");
    }
    Ok((recipe, folder))
}

/// Notes that recipe `id` started a run in `thread`, and returns what the run's agent is asked
/// and the thread's `recipe` mark.
pub async fn started(ctx: &Ctx<'_>, recipe: &Recipe, thread: &str, folder: &str, note: Option<&str>, by: Option<Value>) -> Result<(String, Value)> {
    change(ctx, &recipe.id, |r| {
        r.runs.insert(0, RecipeRun { thread: thread.to_owned(), folder: folder.to_owned(), at: now_ms() });
        r.runs.truncate(KEEP_RUNS);
        Ok(())
    })
    .await?;
    let mut mark = json!({ "id": recipe.id, "name": recipe.name });
    if let Some(by) = by {
        mark["by"] = by;
    }
    Ok((brief(recipe, note), mark))
}

/// What the run's agent is asked: Hub's note on how a recipe works, the recipe and this run's note.
fn brief(recipe: &Recipe, note: Option<&str>) -> String {
    let mut text = format!(
        "(Note from Hub: this is the recipe \"{name}\", a job the user saved to run again and again and \
started on this project now. Do the whole job without stopping to ask, unless something is unclear in a way that \
matters or would destroy work. Work on a new branch, follow the project's own rules (CLAUDE.md, AGENTS.md, docs), \
run its checks, and end at an open pull request; never merge it. Use only packages Customs has cleared \
(customs_search, customs_declare). If there is nothing to do here, say so in one line and change nothing. End \
with a short summary: what you changed, what you checked, what you left and why, and the pull request.)\n\n{prompt}",
        name = recipe.name,
        prompt = recipe.prompt.trim(),
    );
    if let Some(note) = note.map(str::trim).filter(|n| !n.is_empty()) {
        text.push_str(&format!("\n\nThis time, also: {note}"));
    }
    text
}

/// The starter recipes.
pub fn templates() -> Vec<RecipeTemplate> {
    let t = |id: &str, name: &str, about: &str, prompt: &str| RecipeTemplate {
        id: id.into(),
        name: name.into(),
        about: about.into(),
        prompt: prompt.into(),
    };
    vec![
        t(
            "fix-tests",
            "Fix failing tests",
            "Runs the tests and fixes what fails, in the code rather than the tests.",
            "Run this project's test suites (find them in the README, CLAUDE.md, package.json, Cargo.toml, Makefile \
and so on) and fix every failure. Fix the code, not the test, unless the test is clearly what is wrong, and say why \
when it is. Never skip, disable or loosen a test to get green. Run the suites again at the end and show they pass.",
        ),
        t(
            "update-dependencies",
            "Update dependencies",
            "Takes minor and patch updates, checks them against Customs, and lists the major ones.",
            "Update this project's dependencies within their current major versions (`cargo update`, `npm update` and \
so on for each manifest). Look up anything that is new in the lockfile with Hub's customs_search, and declare what \
Customs does not know with customs_declare; leave out what it holds or refuses. Build and run the tests after, and \
back out any single update that breaks them. Do not take major versions: list them at the end with what changed \
and what the upgrade would take.",
        ),
        t(
            "fix-warnings",
            "Fix warnings",
            "Makes the compiler, linter and type checker quiet without hiding anything.",
            "Run the project's compiler, linter and type checker (for example `cargo clippy --all-targets`, \
`npm run check`, `npm run lint`, `tsc --noEmit`) and fix the warnings. Fix the cause rather than silencing it; an \
allow or ignore needs a one-line reason beside it. Keep behaviour the same, and run the tests at the end.",
        ),
        t(
            "add-tests",
            "Add missing tests",
            "Finds the riskiest code with no tests and covers it.",
            "Find the few places where a bug would hurt most and no test would catch it: important logic, error \
paths, parsing, money, permissions. Write focused tests for them in the style the project already uses, and fix \
any bug they find in the same pull request, saying so. Prefer a handful of meaningful tests over many shallow ones.",
        ),
        t(
            "refresh-docs",
            "Bring docs up to date",
            "Makes the README and docs match what the code does today.",
            "Read the README and the docs, then check them against the code: commands that changed, settings and \
endpoints that were added or removed, steps that no longer work. Fix what is wrong or missing, in the docs' own \
voice, and keep it short. Do not change code.",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_brief_ends_at_a_pull_request_and_carries_the_note() {
        let t = &templates()[0];
        let r = Recipe {
            id: "r".into(),
            name: t.name.clone(),
            about: t.about.clone(),
            prompt: t.prompt.clone(),
            engine: None,
            folder: None,
            template: Some(t.id.clone()),
            suggested: None,
            created_at: 0,
            runs: Vec::new(),
        };
        let text = brief(&r, Some(" only the api crate "));
        assert!(text.contains("open pull request; never merge"));
        assert!(text.ends_with("This time, also: only the api crate"));
        assert!(!brief(&r, Some("  ")).contains("This time"));
    }

    #[test]
    fn names_compare_without_case_or_spaces() {
        assert!(same_name(" Fix Tests", "fix tests "));
        assert!(!same_name("Fix tests", "Fix test"));
    }
}
