//! Model types: projects.

use super::*;

/// A folder added as a project, so it shows in the sidebar before it has any threads. Folders
/// threads run in are projects too, without being stored.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Project {
    pub path: String,
    pub name: String,
    #[ts(type = "number")]
    pub created_at: i64,
}

/// One folder on a machine, as the folder picker lists it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FolderEntry {
    pub name: String,
    pub path: String,
    /// Has a `.git`: a repository, so likely a project.
    pub repo: bool,
}

/// The folders in one folder on a machine (`GET /api/folders?path=&host=`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FolderListing {
    /// The folder listed, absolute (`~` expanded).
    pub path: String,
    pub parent: Option<String>,
    /// That machine's home folder, so `~` can be shown for it.
    pub home: String,
    /// `\` on Windows, `/` elsewhere.
    pub sep: String,
    /// The folder listed is a repository itself.
    pub repo: bool,
    /// Drives on Windows, `/` elsewhere.
    pub roots: Vec<String>,
    pub entries: Vec<FolderEntry>,
    /// There were more folders than listed.
    pub more: bool,
}

/// A project found on a machine, for the folder picker to start from: a repository below one of
/// the usual code folders (`GET /api/folders/projects?host=`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectFolder {
    /// The folder's own name.
    pub name: String,
    pub path: String,
    /// The branch checked out, when it is a repository on one.
    pub branch: Option<String>,
    /// `host/owner/repo` from its origin, the same on every machine.
    pub key: Option<String>,
    /// Last commit or checkout there, in ms; 0 when unknown.
    #[ts(type = "number")]
    pub touched: i64,
    /// The branch origin calls its own (`main`, `master`), when this clone knows it. The picker
    /// marks a folder that stands on any other branch.
    #[serde(default)]
    pub main: Option<String>,
    /// Commits on the branch's upstream that this folder does not have yet, as of the last fetch
    /// (nothing is fetched to find out). None when it has no upstream.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub behind: Option<u32>,
    /// When this folder is a linked git worktree: the folder of the repository it belongs to.
    #[serde(default)]
    pub worktree_of: Option<String>,
    /// How many linked worktrees this repository has.
    #[serde(default)]
    pub worktrees: u32,
}

/// The projects on a machine and where they were looked for.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectFolders {
    pub projects: Vec<ProjectFolder>,
    /// The code folders looked in, e.g. `D:\Dev` and `~/Dev`.
    pub roots: Vec<String>,
    /// Where a new project goes: the projects folder.
    pub root: String,
    pub home: String,
    pub sep: String,
}

/// `POST /api/folders`: a folder for a new project on that machine.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NewFolder {
    pub path: String,
    /// Run `git init` in it.
    #[serde(default)]
    pub git: bool,
}

/// Where a repository is on a machine (`GET /api/folders/repo?path=` on the machine that has it,
/// then `?key=` on the one it should run on).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RepoPlaces {
    /// `host/owner/repo`, the same on every machine. None when the folder has no remote.
    pub key: Option<String>,
    /// The remote to clone from elsewhere, without credentials.
    pub url: Option<String>,
    /// Folders on this machine with the repository.
    pub found: Vec<String>,
    /// Where a clone would go on this machine: the projects folder.
    pub root: String,
    /// This machine's home folder, to show paths as `~/…`.
    pub home: String,
}

/// `POST /api/folders/clone`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CloneRepo {
    pub url: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NewProject {
    pub path: String,
    /// Create the folder, run `git init` and start the plan and living docs. Without it the
    /// folder must exist and is only added.
    #[serde(default)]
    #[ts(optional)]
    pub create: Option<bool>,
    /// The name to show, when it differs from the folder's.
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
    /// With `create`: make a private GitHub repository with the GitHub CLI and push to it.
    #[serde(default)]
    #[ts(optional)]
    pub github: Option<bool>,
    /// With `create`: start a thread that picks the stack and scaffolds the project.
    #[serde(default)]
    #[ts(optional)]
    pub setup: Option<bool>,
    /// What the project is, in the user's words, for the setup thread.
    #[serde(default)]
    #[ts(optional)]
    pub about: Option<String>,
    /// With `create`: start from one of the core's templates (`GET /api/projects/templates`).
    /// An id it does not know, or `blank`, writes nothing beyond the standard docs.
    #[serde(default)]
    #[ts(optional)]
    pub template: Option<String>,
    /// With `create` and `github`: make the repository public instead of private.
    #[serde(default)]
    #[ts(optional)]
    pub public: Option<bool>,
    /// With `create`, `github` and a template: add a workflow that builds and tests on every push.
    #[serde(default)]
    #[ts(optional)]
    pub ci: Option<bool>,
}

/// A tool a starter needs, and whether this machine has it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ToolStatus {
    pub tool: String,
    pub found: bool,
    #[serde(default)]
    #[ts(optional)]
    pub version: Option<String>,
}

/// The templates the core can write on this machine, and the tools the stacks need.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectTemplates {
    pub templates: Vec<String>,
    pub tools: Vec<ToolStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CreatedProject {
    pub project: Project,
    /// The setup thread, when one was started.
    pub thread_id: Option<ThreadId>,
    /// Steps that did not work out, e.g. the GitHub repository; the project exists anyway.
    pub notes: Vec<String>,
}

/// Where dev mode is: Hub's own code in a clone of its own, served live to a second window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DevState {
    /// No clone yet.
    Missing,
    /// Cloning and installing packages.
    SettingUp,
    /// The clone is there; the live UI server is not running.
    Stopped,
    Starting,
    Running,
    Failed,
}

/// Dev mode: a clone of Hub's repository whose UI is served with hot reload, so changes made in
/// a thread there show in the dev window as they are saved.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DevStatus {
    /// The clone.
    pub dir: String,
    pub state: DevState,
    /// Where the live UI is served.
    pub url: String,
    pub branch: Option<String>,
    /// Files changed and not yet committed.
    pub changed: u32,
    /// Commits not yet on the branch the updater follows.
    pub ahead: u32,
    /// What is going on, or what went wrong.
    pub message: Option<String>,
    /// The pull request the clone's branch was last shipped to.
    pub pr: Option<String>,
    /// The dev core: Hub's core built from the clone, on a copy of the database, which the dev
    /// window talks to instead of this one.
    pub core: DevCoreState,
    pub core_url: String,
    /// What the dev core is doing, or why its last build failed.
    pub core_message: Option<String>,
    /// When the clone shows the threads' open pull requests on top of the branch the updater
    /// follows, instead of a branch of its own. See `dev::preview`.
    pub preview: Option<DevPreview>,
    /// Whether a new thread in Hub (dev) gets a git worktree of the clone of its own, on a branch
    /// of its own, so threads never edit the same files. See `dev_threads`.
    pub worktrees: bool,
    /// The threads working in worktrees of the clone, newest first.
    pub threads: Vec<DevThread>,
    /// The thread whose worktree the dev window (and the dev core) shows; the clone when left out.
    /// `branch`, `changed`, `ahead` and `pr` above are that folder's.
    pub shown: Option<String>,
    /// The build cache the dev core and the threads share, when last measured, and its cap.
    #[ts(type = "number | null")]
    pub cache_bytes: Option<u64>,
    #[ts(type = "number")]
    pub cache_cap: u64,
}

/// A thread in Hub (dev) and its worktree.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DevThread {
    pub thread_id: String,
    pub title: String,
    pub status: ThreadStatus,
    /// The worktree's folder.
    pub path: String,
    pub branch: String,
    /// Files changed and not yet committed.
    pub changed: u32,
    /// Commits not yet on the branch the updater follows.
    pub ahead: u32,
}

/// What the dev window shows: a thread's worktree, or the clone when `thread` is left out.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DevShow {
    #[ts(optional)]
    pub thread: Option<String>,
}

/// Whether new threads in a project get a git worktree of their own (`project_trees.rs`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectTrees {
    /// The repository the project's folder is in; none when it is not one.
    pub root: Option<String>,
    pub on: bool,
    /// What new threads are cut from, as `remote/branch`, when the project says so.
    pub base: Option<String>,
    /// What Hub cuts them from otherwise: the remote's default branch, when it is known.
    pub default_base: Option<String>,
}

/// Turns worktrees on or off for the project at `path`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectTreesSet {
    pub path: String,
    pub on: bool,
    #[ts(optional)]
    pub base: Option<String>,
}

/// The clone as a preview: the updater's branch with open pull requests merged on top, followed
/// as they are pushed, so their changes show in the dev window long before they are released.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DevPreview {
    /// The one pull request shown; all open ones when left out.
    pub only: Option<u32>,
    /// Live mode in the main window: every open pull request that changes only the pages, so
    /// what it draws runs against the installed core. The others are skipped with why.
    #[serde(default)]
    pub live: bool,
    /// The updater's branch the preview starts from, as a short hash.
    pub base: Option<String>,
    /// Merged into the preview.
    pub included: Vec<DevPreviewPr>,
    /// Open, but left out, with why.
    pub skipped: Vec<DevPreviewPr>,
    /// Every open pull request, to pick one from.
    pub open: Vec<DevPreviewPr>,
    /// Some of the included ones change the core, which the dev window only runs with a dev core.
    pub core_changed: bool,
    pub refreshing: bool,
    #[ts(type = "number | null")]
    pub refreshed_at: Option<i64>,
    /// Why the preview is not current, e.g. files changed in the clone or GitHub not answering.
    pub message: Option<String>,
    /// Pull requests merged lately that the installed version does not have yet. The live pages
    /// already show them (they are in the branch they are built on); `why` says where the update
    /// stands. A core from before this leaves the field out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<DevPreviewPr>>", optional)]
    pub merged: Vec<DevPreviewPr>,
    /// What the last look for pushes found; `None` before the first one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub check: Option<DevPreviewCheck>,
}

/// One look at GitHub for pushes: when, and what moved since the preview was built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DevPreviewCheck {
    #[ts(type = "number")]
    pub at: i64,
    /// Open pull requests that are new or have a new head since the preview was built.
    pub pushed: Vec<u32>,
    /// The branch the updater follows moved (something was merged).
    pub base_moved: bool,
    /// Why the look failed, when it did.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DevPreviewPr {
    pub number: u32,
    pub title: String,
    pub url: String,
    /// Why it was left out.
    pub why: Option<String>,
    /// The files it changes, for the ones Live mode shows, so a page that breaks can say which
    /// pull request it came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub files: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DevPreviewSet {
    /// Off goes back to the clone's own branch.
    pub on: bool,
    /// One pull request only; all open ones when left out.
    pub only: Option<u32>,
    /// Refresh now, also when files changed in the clone: they are parked on a branch first.
    #[serde(default)]
    pub refresh: bool,
    /// Live mode: the open pull requests that change only the pages (see `DevPreview::live`).
    /// Sets the clone up first when it is not there yet.
    #[serde(default)]
    #[ts(optional)]
    pub live: Option<bool>,
}

/// Where the dev core is. It is rebuilt and restarted when a Rust file in the clone is saved;
/// a build that fails leaves the previous one running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DevCoreState {
    /// The dev window talks to this core.
    #[default]
    Off,
    Building,
    Running,
    Failed,
}

/// Commits what dev mode changed to a branch of its own and opens a pull request for it, which
/// Hub's merge queue merges.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DevShip {
    /// Written by the small model from the diff when left out.
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DevShipped {
    pub commit: String,
    pub message: String,
    pub branch: String,
    pub pr: Option<String>,
    /// The pull request was there already and got the new commits.
    pub updated: bool,
}

/// Where new projects go, and whether a GitHub repository can be made for them.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectDefaults {
    pub root: String,
    /// The GitHub CLI is installed and logged in.
    pub github: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SetProjectRoot {
    pub root: String,
}
