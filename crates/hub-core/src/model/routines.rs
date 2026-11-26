//! Model types: routines.

use super::*;

/// How often a routine runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Every {
    /// Every day at `at`.
    #[default]
    Day,
    /// Monday to Friday at `at`.
    Weekdays,
    /// Once a week, on `weekday` at `at`.
    Week,
    /// Every `hours` hours.
    Hours,
}

/// When a routine runs, in the time zone the app last reported.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Schedule {
    pub every: Every,
    /// Minutes after midnight.
    #[serde(default)]
    pub at: u32,
    /// 0 is Monday, 6 Sunday; for `week`.
    #[serde(default)]
    pub weekday: u8,
    /// For `hours`.
    #[serde(default)]
    pub hours: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RunState {
    Working,
    Done,
    Failed,
}

/// One run of a routine: the thread it started and what it reported.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RoutineRun {
    pub thread: Option<ThreadId>,
    #[ts(type = "number")]
    pub at: i64,
    /// "schedule", or "you" for Run now.
    pub by: String,
    pub state: RunState,
    /// The run's last reply, or why it failed.
    #[serde(default)]
    pub report: String,
    #[ts(type = "number | null")]
    pub ended_at: Option<i64>,
}

/// Work Hub starts by itself on a schedule (`routines.rs`). A run only reports; fixing is up to
/// the user, who answers in the run's thread.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Routine {
    pub id: String,
    pub name: String,
    /// The folder it runs in, on this machine.
    pub folder: String,
    /// None is the default engine.
    pub engine: Option<String>,
    pub prompt: String,
    pub schedule: Schedule,
    pub enabled: bool,
    /// The starter routine it was made from.
    pub template: Option<String>,
    /// When the schedule was last set; a run is due at the first slot after this or the last run.
    #[ts(type = "number")]
    pub since: i64,
    /// Newest first, the last few.
    #[serde(default)]
    pub runs: Vec<RoutineRun>,
    /// When it runs next; None when off.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub next_at: Option<i64>,
}

/// A card for the Overview that an agent writes on request (`widgets.rs`): a page shown by a
/// background thread, framed on the board like any card.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Widget {
    pub id: String,
    /// The page's title once there is one, a short form of the ask before.
    pub title: String,
    /// What the user asked for, as written or dictated.
    pub ask: String,
    /// The thread that builds it and takes changes.
    pub thread: Option<String>,
    /// The page in that thread the card shows: the newest one it showed.
    pub page: Option<String>,
    pub state: WidgetState,
    /// Why it failed, when it did.
    #[serde(default)]
    pub error: String,
    #[ts(type = "number")]
    pub created: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum WidgetState {
    /// The thread is writing it, or a change to it.
    Building,
    Ready,
    /// The turn ended without a page.
    Failed,
}

/// `POST /api/widgets` and `POST /api/widgets/{id}/change`: what the user wants.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WidgetAsk {
    pub text: String,
}

/// A routine as the user writes it (`POST /api/routines`, `PUT /api/routines/{id}`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RoutineDraft {
    pub name: String,
    pub folder: String,
    #[serde(default)]
    #[ts(optional)]
    pub engine: Option<String>,
    pub prompt: String,
    pub schedule: Schedule,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    #[ts(optional)]
    pub template: Option<String>,
}

/// A starter routine Hub ships, added to a folder with one click.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RoutineTemplate {
    pub id: String,
    pub name: String,
    /// One line on what it looks for.
    pub about: String,
    pub prompt: String,
    pub schedule: Schedule,
}

/// `GET /api/routines`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Routines {
    pub routines: Vec<Routine>,
    pub templates: Vec<RoutineTemplate>,
}

/// One run of a recipe: the thread it started and where. How it went is that thread's status.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RecipeRun {
    pub thread: ThreadId,
    pub folder: String,
    #[ts(type = "number")]
    pub at: i64,
}

/// Who suggested a recipe the user has not kept yet: an agent in a thread, or one outside Hub.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RecipeSuggestion {
    /// The thread's title, or the agent key's name.
    pub by: String,
    /// The thread it came from, when it came from one.
    pub thread: Option<ThreadId>,
    /// Why the agent thinks it is worth keeping.
    #[serde(default)]
    pub why: String,
}

/// A saved job the user starts by hand on any project (`recipes.rs`), like "update the
/// dependencies" or "fix the failing tests". Unlike a routine it runs only when started, and it
/// does the work: a run is an ordinary thread that ends at an open pull request.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Recipe {
    pub id: String,
    pub name: String,
    /// One line on what it does.
    #[serde(default)]
    pub about: String,
    pub prompt: String,
    /// None is the default engine.
    pub engine: Option<String>,
    /// The folder it always runs in; None runs it on whichever project is picked.
    pub folder: Option<String>,
    /// The starter recipe it was made from.
    pub template: Option<String>,
    /// Set while an agent's suggestion waits for the user to keep or dismiss it.
    #[serde(default)]
    pub suggested: Option<RecipeSuggestion>,
    #[ts(type = "number")]
    pub created_at: i64,
    /// Newest first, the last few.
    #[serde(default)]
    pub runs: Vec<RecipeRun>,
}

/// A recipe as the user writes it (`POST /api/recipes`, `PUT /api/recipes/{id}`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RecipeDraft {
    pub name: String,
    #[serde(default)]
    #[ts(optional)]
    pub about: Option<String>,
    pub prompt: String,
    #[serde(default)]
    #[ts(optional)]
    pub engine: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub folder: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub template: Option<String>,
}

/// A starter recipe Hub ships, kept with one click.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RecipeTemplate {
    pub id: String,
    pub name: String,
    pub about: String,
    pub prompt: String,
}

/// `GET /api/recipes`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Recipes {
    pub recipes: Vec<Recipe>,
    pub templates: Vec<RecipeTemplate>,
}

/// `POST /api/recipes/{id}/run`: where to run it, and anything to add this time.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RunRecipe {
    /// Ignored for a recipe tied to a folder.
    #[serde(default)]
    #[ts(optional)]
    pub folder: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub note: Option<String>,
}
