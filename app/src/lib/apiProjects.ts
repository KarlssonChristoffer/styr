// The API calls about projects, folders, machines and settings (part of `api`, see api.ts).
import type { Standing } from "./bindings/Standing";
import type { Slip } from "./bindings/Slip";
import type { WaitingChange } from "./bindings/WaitingChange";
import type { HarnessSettings } from "./bindings/HarnessSettings";
import type { Output } from "./bindings/Output";
import type { InboxIdea } from "./bindings/InboxIdea";
import type { IdeaPlace } from "./bindings/IdeaPlace";
import type { MoveIdea } from "./bindings/MoveIdea";
import type { NewIdea } from "./bindings/NewIdea";
import type { HandoffBrief } from "./bindings/HandoffBrief";
import type { OwnTool } from "./bindings/OwnTool";
import type { OpenTerminal } from "./bindings/OpenTerminal";
import type { TerminalSession } from "./bindings/TerminalSession";
import type { OwnToolRan } from "./bindings/OwnToolRan";
import type { RunOwnToolAction } from "./bindings/RunOwnToolAction";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import type { HostSnapshot } from "./bindings/HostSnapshot";
import type { Thread } from "./bindings/Thread";
import type { WorkerRun } from "./bindings/WorkerRun";
import type { RunWorkerControl } from "./bindings/RunWorkerControl";
import type { CredentialCheck } from "./bindings/CredentialCheck";
import type { EngineCatalog } from "./bindings/EngineCatalog";
import type { EngineDraft } from "./bindings/EngineDraft";
import type { EngineTest } from "./bindings/EngineTest";
import type { LoginJob } from "./bindings/LoginJob";
import type { PendingSignIn } from "./bindings/PendingSignIn";
import type { InstallJob } from "./bindings/InstallJob";
import type { MachineEngines } from "./bindings/MachineEngines";
import type { FileText } from "./bindings/FileText";
import type { Database } from "./bindings/Database";
import type { Workbook } from "./bindings/Workbook";
import type { DocPage } from "./bindings/DocPage";
import type { SearchHit } from "./bindings/SearchHit";
import type { VaultMap } from "./bindings/VaultMap";
import type { GithubInfo } from "./bindings/GithubInfo";
import type { Focus } from "./bindings/Focus";
import type { FocusEdit } from "./bindings/FocusEdit";
import type { FocusSummary } from "./bindings/FocusSummary";
import type { PlanEdit } from "./bindings/PlanEdit";
import type { PlanItem } from "./bindings/PlanItem";
import type { ProjectInfo } from "./bindings/ProjectInfo";
import type { ProjectTrees } from "./bindings/ProjectTrees";
import type { ProjectRuntime } from "./bindings/ProjectRuntime";
import type { NewProject } from "./bindings/NewProject";
import type { TreeEntry } from "./bindings/TreeEntry";
import type { About } from "./bindings/About";
import type { Accounts } from "./bindings/Accounts";
import type { SshInfo } from "./bindings/SshInfo";
import type { CoreSettings } from "./bindings/CoreSettings";
import type { ConfigEdit } from "./bindings/ConfigEdit";
import type { ToolInfo } from "./bindings/ToolInfo";
import type { DiskUsage } from "./bindings/DiskUsage";
import type { Cleanup } from "./bindings/Cleanup";
import type { ReleaseNotes } from "./bindings/ReleaseNotes";
import type { CliNews } from "./bindings/CliNews";
import type { GuardPrefs } from "./bindings/GuardPrefs";
import type { PermissionPrefs } from "./bindings/PermissionPrefs";
import type { Workers } from "./bindings/Workers";
import type { HostInfo } from "./bindings/HostInfo";
import type { NewHost } from "./bindings/NewHost";
import type { Tailnet } from "./bindings/Tailnet";
import type { SetUpAnswer } from "./bindings/SetUpAnswer";
import type { LanMachine } from "./bindings/LanMachine";
import type { Device } from "./bindings/Device";
import type { PairCode } from "./bindings/PairCode";
import type { PhoneLink } from "./bindings/PhoneLink";
import type { InstallAnswer } from "./bindings/InstallAnswer";
import type { InstallRequest } from "./bindings/InstallRequest";
import type { DevStatus } from "./bindings/DevStatus";
import type { DevShipped } from "./bindings/DevShipped";
import type { BenchAsk } from "./bindings/BenchAsk";
import type { BenchRun } from "./bindings/BenchRun";
import type { BenchSuiteInfo } from "./bindings/BenchSuiteInfo";
import type { ProjectServices } from "./bindings/ProjectServices";
import type { ServicesDone } from "./bindings/ServicesDone";
import type { TerminalOpened } from "./bindings/TerminalOpened";
import type { MachineAbout } from "./bindings/MachineAbout";
import type { OpenOnMachine } from "./bindings/OpenOnMachine";
import type { EngineCli } from "./bindings/EngineCli";
import type { FolderListing } from "./bindings/FolderListing";
import type { GithubConnect } from "./bindings/GithubConnect";
import type { GithubRepos } from "./bindings/GithubRepos";
import type { ProjectFolders } from "./bindings/ProjectFolders";
import type { RepoPlaces } from "./bindings/RepoPlaces";
import type { Halt } from "./bindings/Halt";
import type { Resumed } from "./bindings/Resumed";
import type { BringProject } from "./bindings/BringProject";
import type { Left } from "./bindings/Left";
import type { MoveThread } from "./bindings/MoveThread";
import type { MovedThread } from "./bindings/MovedThread";
import type { Brought } from "./bindings/Brought";
import type { GitIdentity } from "./bindings/GitIdentity";
import type { MachineReady } from "./bindings/MachineReady";
import type { SimView } from "./bindings/SimView";
import type { Fleet } from "./bindings/Fleet";
import { quiet } from "./quiet";
import { BASE } from "./home";
import { ApiError } from "./compat";
import { call, upload, at, on, p } from "./apiCall";

/** What a new project can ask for beyond a folder and a name; a hubd without feature `project.templates` leaves it out. */
export type NewProjectMore = Pick<NewProject, "template" | "public" | "ci">;

export const projectApi = {
  // Folders on the machine a thread will run on (`folders.rs`); `host` asks that machine.
  folders: (path: string, host?: string | null, hidden = false) =>
    call<FolderListing>("GET", `/api/folders?path=${encodeURIComponent(path)}${hidden ? "&hidden=true" : ""}${at(host)}`),
  /** The repositories in the usual code folders on that machine, newest first. */
  projectFolders: (host?: string | null) => call<ProjectFolders>("GET", `/api/folders/projects${host ? `?host=${encodeURIComponent(host)}` : ""}`),
  makeFolder: (path: string, git: boolean, host?: string | null) =>
    call<string>("POST", `/api/folders${host ? `?host=${encodeURIComponent(host)}` : ""}`, { path, git }),
  /** The repository a folder holds, as a key that is the same on every machine. */
  repoOf: (path: string, host?: string | null) =>
    call<RepoPlaces>("GET", `/api/folders/repo?path=${encodeURIComponent(path)}${at(host)}`),
  /** Where the repository with `key` is on a machine, and where a clone would go. */
  findRepo: (key: string, host?: string | null) =>
    call<RepoPlaces>("GET", `/api/folders/repo?key=${encodeURIComponent(key)}${at(host)}`),
  /** Every repository of the GitHub account `gh` is signed in as on that machine (feature `github.repos`). */
  githubRepos: (host?: string | null, fresh = false) =>
    call<GithubRepos>("GET", `/api/github/repos?fresh=${fresh}${at(host)}`),
  /** Where that machine stands with GitHub; `start` runs `gh auth login --web` there and answers with the code. */
  githubConnect: (host?: string | null) => call<GithubConnect>("GET", `/api/github/connect${host ? `?host=${encodeURIComponent(host)}` : ""}`),
  githubConnectStart: (host?: string | null) => call<GithubConnect>("POST", `/api/github/connect${host ? `?host=${encodeURIComponent(host)}` : ""}`),
  /** Signs that machine out of GitHub (feature `github.account`); answers with the state after. */
  githubDisconnect: (host?: string | null) => call<GithubConnect>("POST", `/api/github/disconnect${host ? `?host=${encodeURIComponent(host)}` : ""}`),
  githubConnectCancel: (host?: string | null) => call<GithubConnect>("DELETE", `/api/github/connect${host ? `?host=${encodeURIComponent(host)}` : ""}`),
  cloneRepo: (url: string, path: string, host?: string | null) =>
    call<string>("POST", `/api/folders/clone${host ? `?host=${encodeURIComponent(host)}` : ""}`, { url, path }),
  /** Carries a project to another machine with its history and env files; the machine it
   * goes to needs no GitHub login. */
  /** The same thread carries on on another machine, with its code and uncommitted changes. */
  moveThread: (move: MoveThread) => call<MovedThread>("POST", "/api/threads/move", move),
  /** Leaving: everything working here moves to a machine that stays on. */
  leaving: (to: string) => call<Left>("POST", "/api/threads/leaving", { to }),
  bringProject: (bring: BringProject) => call<Brought>("POST", "/api/folders/bring", bring),
  project: (path: string, host?: string | null) =>
    call<ProjectInfo>("GET", `/api/project?path=${encodeURIComponent(path)}${at(host)}`),
  projectGithub: (path: string, host?: string | null) =>
    call<GithubInfo>("GET", `/api/project/github?path=${encodeURIComponent(path)}${at(host)}`),
  projectRuntime: (path: string, host?: string | null) =>
    call<ProjectRuntime>("GET", `/api/project/runtime?path=${encodeURIComponent(path)}${at(host)}`),
  projectTree: (path: string, rel = "", host?: string | null) =>
    call<TreeEntry[]>("GET", `/api/project/tree?path=${encodeURIComponent(path)}&rel=${encodeURIComponent(rel)}${at(host)}`),
  /** The rules the project's agents are given, each with when it last changed. */
  projectRules: (path: string, host?: string | null) => call<Standing>("GET", `/api/project/rules?${p(path)}${at(host)}`),
  /** What the project's threads made, where each thing is now and what happened to it. */
  projectOutputs: (path: string, host?: string | null) => call<Output[]>("GET", `/api/project/outputs?${p(path)}${at(host)}`),
  /** One picture of the project's library as a blob URL for an `<img>` (fetched, so the core
   *  sees who asks); revoke it when it is not shown any more. */
  outputPicture: async (path: string, id: string, host?: string | null) => {
    const res = await fetch(`${BASE}/api/project/outputs/raw?path=${encodeURIComponent(path)}&rel=${encodeURIComponent(id)}${at(host)}`);
    if (!res.ok) throw new Error((await quiet(res.json(), "error body", null))?.error ?? res.statusText);
    return URL.createObjectURL(await res.blob());
  },
  /** The text of one thing in the project's library, by its id, to read it inside Hub. */
  outputText: (path: string, id: string, host?: string | null) =>
    call<FileText>("GET", `/api/project/outputs/text?path=${encodeURIComponent(path)}&rel=${encodeURIComponent(id)}${at(host)}`),
  /** A picture, PDF, sound or video file in a folder as a blob address for the readers (fetched,
   *  so a paired machine's token goes along); revoke it when it is not shown any more. */
  projectRaw: async (path: string, rel: string, host?: string | null) => {
    const res = await fetch(`${BASE}/api/project/raw?path=${encodeURIComponent(path)}&rel=${encodeURIComponent(rel)}${at(host)}`);
    if (!res.ok) throw new Error((await quiet(res.json(), "error body", null))?.error ?? res.statusText);
    const blob = await res.blob();
    return { url: URL.createObjectURL(blob), size: blob.size };
  },
  /** A spreadsheet as one table per sheet (feature `view.sheet`). */
  projectSheet: (path: string, rel: string, host?: string | null) =>
    call<Workbook>("GET", `/api/project/sheet?path=${encodeURIComponent(path)}&rel=${encodeURIComponent(rel)}${at(host)}`),
  /** A SQLite file as its tables (feature `view.database`). */
  projectDatabase: (path: string, rel: string, host?: string | null) =>
    call<Database>("GET", `/api/project/database?path=${encodeURIComponent(path)}&rel=${encodeURIComponent(rel)}${at(host)}`),
  /** What Hub sent to an AI for a thread, newest first (feature `package.ledger`). */
  packageSlips: (thread: string, purpose?: string, limit = 50) =>
    call<Slip[]>("GET", `/api/package/slips?thread=${encodeURIComponent(thread)}${purpose ? `&purpose=${encodeURIComponent(purpose)}` : ""}&limit=${limit}`),
  projectFile: (path: string, rel: string, host?: string | null) =>
    call<FileText>("GET", `/api/project/file?path=${encodeURIComponent(path)}&rel=${encodeURIComponent(rel)}${at(host)}`),
  /** Runs an HTML file as a live page (feature `pages.file`); the page is then at `pageUrl(thread, page)`. */
  projectPage: (path: string, rel: string) => call<{ thread: string; page: string }>("POST", "/api/project/page", { path, rel }),
  vault: (path: string, host?: string | null) =>
    call<VaultMap>("GET", `/api/project/vault?path=${encodeURIComponent(path)}${at(host)}`),
  /** A doc by path, or by name the way a [[link]] says it. */
  vaultDoc: (path: string, doc: string, host?: string | null) =>
    call<DocPage>("GET", `/api/project/vault/doc?path=${encodeURIComponent(path)}&rel=${encodeURIComponent(doc)}${at(host)}`),
  vaultSearch: (path: string, q: string, host?: string | null) =>
    call<SearchHit[]>("GET", `/api/project/vault/search?path=${encodeURIComponent(path)}&q=${encodeURIComponent(q)}${at(host)}`),
  /** Writes a doc the user edited. */
  saveDoc: (path: string, rel: string, body: string, host?: string | null) =>
    call<void>("PUT", `/api/project/vault/doc${on(host)}`, { path, rel, body }),
  /** The library's folder, after Hub brought its facts up to date. */
  library: () => call<{ root: string }>("GET", "/api/library"),
  /** The first message of a thread that answers from the library, or teaches one note and checks it stuck. */
  libraryPrompt: (q: { ask: string } | { learn: string }) =>
    call<{ message: string }>("GET", `/api/library/prompt?${"ask" in q ? `ask=${encodeURIComponent(q.ask)}` : `learn=${encodeURIComponent(q.learn)}`}`),
  editPlan: (edit: PlanEdit, host?: string | null) =>
    call<PlanItem[]>("POST", `/api/project/plan${host ? `?host=${encodeURIComponent(host)}` : ""}`, edit),
  editFocus: (edit: FocusEdit, host?: string | null) =>
    call<Focus>("POST", `/api/project/focus${host ? `?host=${encodeURIComponent(host)}` : ""}`, edit),
  /** Each folder's focus in a line, by path; folders without one are left out. */
  focusSummaries: (paths: string[]) => call<Record<string, FocusSummary>>("POST", "/api/project/focus/summaries", { paths }),
  /** The idea inbox, newest first. */
  ideas: () => call<InboxIdea[]>("GET", "/api/ideas"),
  /** Keeps an idea; it comes back sorting, and the stream says where it went. */
  captureIdea: (idea: NewIdea) => call<InboxIdea>("POST", "/api/ideas", idea),
  ideaPlaces: () => call<IdeaPlace[]>("GET", "/api/ideas/places"),
  moveIdea: (id: string, to: MoveIdea) => call<InboxIdea>("POST", `/api/ideas/${encodeURIComponent(id)}/move`, to),
  /** Takes the idea out of its focus file and the inbox. */
  dropIdea: (id: string) => call<void>("DELETE", `/api/ideas/${encodeURIComponent(id)}`),
  /** The first message of a thread that drafts docs/FOCUS.md. */
  focusDraft: () => call<{ message: string }>("GET", "/api/project/focus/draft"),
  openFolder: (path: string, how: "editor" | "terminal" | "files") =>
    call<void>("POST", "/api/project/open", { path, with: how }),
  /** Whether new threads in a project get a worktree of their own (feature `project.worktrees`). */
  projectTrees: (path: string) => call<ProjectTrees>("GET", `/api/projects/trees?path=${encodeURIComponent(path)}`),
  setProjectTrees: (path: string, on: boolean) => call<ProjectTrees>("PUT", "/api/projects/trees", { path, on }),
  setUpDocs: (path: string, host?: string | null) =>
    call<void>("POST", `/api/project/docs${host ? `?host=${encodeURIComponent(host)}` : ""}`, { path }),
  dev: () => call<DevStatus>("GET", "/api/dev"),
  /** Clones Hub to `dir` (default: Hub under the projects folder) and starts the live UI. */
  devSetup: (dir?: string) => call<DevStatus>("POST", "/api/dev/setup", { dir }),
  devStart: () => call<DevStatus>("POST", "/api/dev/start"),
  devStop: () => call<DevStatus>("POST", "/api/dev/stop"),
  /** Commits the clone's changes to a branch and opens a pull request (or adds to the open one);
   *  an empty message is written by the small model. */
  devShip: (message?: string) => call<DevShipped>("POST", "/api/dev/ship", { message: message || null }),
  /** Points the dev window at a core built from the clone (rebuilt on save), or back at this one. */
  devCore: (on: boolean) => call<DevStatus>("POST", "/api/dev/core", { on }),
  /** The clone follows the open pull requests merged on top of main (or just `only`), refreshed as
   *  they are pushed; off puts it back on main. `refresh` refreshes now, parking changed files.
   *  `live` is Live mode's: every open one that changes only the pages, setting dev mode up first
   *  when it is not (feature `dev.live`). */
  devPreview: (on: boolean, only: number | null = null, refresh = false, live = false) =>
    call<DevStatus>("POST", "/api/dev/preview", live ? { on, only, refresh, live } : { on, only, refresh }),
  /** Shows a thread's worktree in the dev window (and the dev core), or the clone with `null`. */
  devShow: (thread: string | null) => call<DevStatus>("POST", "/api/dev/show", { thread }),
  /** Whether new threads in Hub (dev) each get a worktree of their own. */
  devWorktrees: (on: boolean) => call<DevStatus>("POST", "/api/dev/worktrees", { on }),
  /** Removes a thread's worktree; uncommitted work is kept in git, a branch with commits stays. */
  devRemoveWorktree: (thread: string) => call<DevStatus>("POST", "/api/dev/worktrees/remove", { thread }),
  /** Starts the inbox simulator's fake threads; returns how many. */
  simStart: () => call<number>("POST", "/api/sim"),
  /** Removes every simulated thread; returns how many went. */
  simClear: () => call<number>("DELETE", "/api/sim"),
  /** Starts a bench run (feature `bench.runs`); refused in words when the plan is too full or a run is going. */
  benchStart: (ask: BenchAsk) => call<BenchRun>("POST", "/api/bench/runs", ask),
  /** Every bench run, newest first, without their rows. */
  benchRuns: () => call<BenchRun[]>("GET", "/api/bench/runs"),
  /** One bench run with its rows and its markdown report. */
  benchRun: (id: string) => call<{ run: BenchRun; report: string | null }>("GET", `/api/bench/runs/${encodeURIComponent(id)}`),
  /** Ends the run once the repeat it is in has ended. */
  benchStop: (id: string) => call<{ stopping: boolean; note: string }>("POST", `/api/bench/runs/${encodeURIComponent(id)}/stop`),
  /** The built-in suites with their defaults and estimates from earlier runs (feature `bench.suites`). */
  benchSuites: () => call<BenchSuiteInfo[]>("GET", "/api/bench/suites"),
  openUrl: (url: string) => call<void>("POST", "/api/open", { url }),
  reveal: (path: string) => call<void>("POST", "/api/reveal", { path }),
  /** Saves a file in Downloads on the machine the core runs on, and says where it went. */
  download: (name: string, file: Blob) => upload(`/api/downloads?name=${encodeURIComponent(name)}`, file),
  /** Adds a picture to a project's `docs/images`, and says where it went. */
  addImage: (project: string, name: string, file: Blob) =>
    upload(`/api/project/images?path=${encodeURIComponent(project)}&name=${encodeURIComponent(name)}`, file),
  version: (host?: string | null) => call<UpdateStatus>("GET", `/api/version${on(host)}`),
  about: () => call<About>("GET", "/api/prefs/about"),
  accounts: () => call<Accounts>("GET", "/api/prefs/accounts"),
  ssh: () => call<SshInfo>("GET", "/api/prefs/ssh"),
  guard: () => call<GuardPrefs>("GET", "/api/prefs/guard"),
  setGuard: (prefs: GuardPrefs) => call<GuardPrefs>("PUT", "/api/prefs/guard", prefs),
  permissions: () => call<PermissionPrefs>("GET", "/api/prefs/permissions"),
  setPermissions: (prefs: PermissionPrefs) => call<PermissionPrefs>("PUT", "/api/prefs/permissions", prefs),
  /** The core's settings, or those of the hubd on `host`. */
  coreSettings: (host?: string | null) => call<CoreSettings>("GET", `/api/prefs/config${on(host)}`),
  editConfig: (edit: ConfigEdit, host?: string | null) => call<CoreSettings>("PUT", `/api/prefs/config${on(host)}`, edit),
  restartCore: (host?: string | null) => call<void>("POST", `/api/prefs/restart${on(host)}`),
  /** Settings > Engines: harnesses, providers and the engines made of them, on `host` or here. */
  engineCatalog: (host?: string | null) => call<EngineCatalog>("GET", `/api/engines${on(host)}`),
  /** A harness's own settings (Hermes's tools, memory…), read and changed through its own program. */
  harnessSettings: (harness: string, host?: string | null) =>
    call<HarnessSettings>("GET", `/api/harnesses/${encodeURIComponent(harness)}/settings${on(host)}`),
  setHarnessSetting: (harness: string, key: string, value: unknown, host?: string | null) =>
    call<HarnessSettings>("PUT", `/api/harnesses/${encodeURIComponent(harness)}/settings${on(host)}`, { key, value }),
  testEngine: (draft: EngineDraft, host?: string | null) => call<EngineTest>("POST", `/api/engines/test${on(host)}`, draft),
  addEngine: (draft: EngineDraft, host?: string | null) => call<EngineCatalog>("POST", `/api/engines${on(host)}`, draft),
  /** How the engines added here stand on each other machine, and sending them there now. */
  engineMachines: () => call<MachineEngines[]>("GET", "/api/engines/machines"),
  shareEngines: () => call<MachineEngines[]>("POST", "/api/engines/share"),
  /** Signs an engine in on a machine with a link and a code shown here (`login.rs`). */
  startLogin: (engine: string, host?: string | null) =>
    call<LoginJob>("POST", `/api/engines/${encodeURIComponent(engine)}/login${on(host)}`),
  /** Sign-ins waiting for a code on the other machines (`autologin.rs`); none from a hubd too old to know. */
  pendingSignIns: () => call<PendingSignIn[]>("GET", "/api/sign-ins/pending").catch(() => [] as PendingSignIn[]),
  loginJob: (engine: string, host?: string | null) =>
    call<LoginJob | null>("GET", `/api/engines/${encodeURIComponent(engine)}/login${on(host)}`),
  installJobs: (host?: string | null) => call<InstallJob[]>("GET", `/api/engines/install${on(host)}`),
  installHarness: (harness: string, host?: string | null) => call<InstallJob>("POST", `/api/engines/install${on(host)}`, { harness }),
  removeEngine: (name: string, host?: string | null) =>
    call<EngineCatalog>("DELETE", `/api/engines/${encodeURIComponent(name)}${on(host)}`),
  tools: () => call<ToolInfo[]>("GET", "/api/prefs/tools"),
  /** Settings > Updates: hubd and every agent CLI on every machine, and updating one everywhere. */
  updates: (fresh = false) => call<Fleet>("GET", `/api/updates${fresh ? "?fresh=true" : ""}`),
  /** Updates one CLI (or "hubd") on every machine online, or only on `machine`. */
  updateEverywhere: (tool: string, machine?: string) => call<Fleet>("POST", "/api/updates/run", { tool, machine }),
  /** An agent CLI's own release notes for the versions after `from` up to `to`. */
  cliNews: (tool: string, from?: string | null, to?: string | null) =>
    call<CliNews>("GET", `/api/updates/news?${new URLSearchParams({ tool, ...(from ? { from } : {}), ...(to ? { to } : {}) })}`),
  followUpdates: (on: boolean) => call<Fleet>("PUT", "/api/updates/follow", { on }),
  notes: () => call<ReleaseNotes[]>("GET", "/api/prefs/notes"),
  installUpdate: (host?: string | null) => call<void>("POST", `/api/update/install${on(host)}`),
  retryUpdate: (host?: string | null) => call<void>("POST", `/api/update/retry${on(host)}`),
  /** The new pages have rendered, so a version on probation is alive (`update.alive`). */
  updateAlive: (version: string) => call<void>("POST", "/api/update/alive", { version }),
  /** Closes the "went back" notice (`update.alive`). */
  dismissRolledBack: () => call<void>("DELETE", "/api/update/rolled-back"),
  /** Builds the version that waits for a quiet branch now (`update.quiet`). */
  buildUpdateNow: () => call<void>("POST", "/api/update/build-now"),
  /** What is merged and not yet in the running Hub, newest first (`update.modes`). */
  updateWaiting: () => call<WaitingChange[]>("GET", "/api/update/waiting"),
  disk: () => call<DiskUsage>("GET", "/api/prefs/disk"),
  freeDisk: () => call<DiskUsage>("POST", "/api/prefs/disk"),
  /** What the last clean-up after agent work removed and left; `cleanUp` runs one now. */
  cleanup: () => call<Cleanup | null>("GET", "/api/cleanup"),
  cleanUp: () => call<Cleanup>("POST", "/api/cleanup"),
  writeNotes: () => call<ReleaseNotes>("POST", "/api/prefs/notes"),
  /** What a machine is doing: this one, or another by name. */
  host: (host?: string | null) => call<HostSnapshot>("GET", `/api/host${on(host)}`),
  /** What a machine is: board, OS, uptime, disk, temperature, Tailscale. */
  hostAbout: (host?: string | null) => call<MachineAbout>("GET", `/api/host/about${on(host)}`),
  /** Whether a machine is ready for work, each check with its fix. */
  checkCredentials: (host?: string | null) => call<CredentialCheck>("POST", `/api/machine/credentials/check${on(host)}`),
  machineReady: (host?: string | null) => call<MachineReady>("GET", `/api/machine/ready${on(host)}`),
  /** A Mac's iPhone simulators and the live picture's phase (feature `sim.view`, a Mac's hubd). */
  simView: (host?: string | null) => call<SimView>("GET", `/api/simview${on(host)}`),
  /** Starts (with a simulator's id) or stops the live picture; the answer says how far it got. */
  simLive: (udid: string | null, onOff: boolean, host?: string | null) =>
    call<SimView>("POST", `/api/simview/live${on(host)}`, { udid, on: onOff }),
  /** The newest picture of a simulator as a blob URL for an `<img>`; null while the live picture has none yet.
   *  Revoke it when it is not shown any more. */
  simFrame: async (udid: string, host?: string | null): Promise<string | null> => {
    const res = await fetch(`${BASE}/api/simview/frame?udid=${encodeURIComponent(udid)}${at(host)}`);
    if (res.status === 204) return null;
    if (!res.ok) throw new ApiError((await quiet(res.json(), "error body", null))?.error ?? res.statusText, res.status);
    return URL.createObjectURL(await res.blob());
  },
  /** Touches (`begin`, `move`, `end`; the place as 0..1 of the screen from its top left) and a button
   *  (`home`, `swipe_home`, `app_switcher`) for the simulator that is watched live. */
  simInput: (udid: string, input: { touches?: { phase: "begin" | "move" | "end"; x: number; y: number }[]; button?: string }, host?: string | null) =>
    call<void>("POST", `/api/simview/input${on(host)}`, { udid, ...input }),
  /** Boots a simulator and answers when it is ready. */
  simBoot: (udid: string, host?: string | null) => call<SimView>("POST", `/api/simview/boot${on(host)}`, { udid }),
  /** Shuts a simulator down (the live picture of it stops first). */
  simShutdown: (udid: string, host?: string | null) => call<SimView>("POST", `/api/simview/shutdown${on(host)}`, { udid }),
  /** Hub's own iPhone app in a Mac's simulator: start it paired with that machine, open a thread, or start it on a tab. */
  simApp: (udid: string, app: { action: "start" | "thread" | "tab"; thread?: string; tab?: number }, host?: string | null) =>
    call<void>("POST", `/api/simview/app${on(host)}`, { udid, ...app }),
  setGitIdentity: (id: GitIdentity, host?: string | null) => call<void>("POST", `/api/machine/git-identity${on(host)}`, id),
  /** Opens a terminal window here, logged in on that machine. */
  openOnMachine: (name: string, open: OpenOnMachine) =>
    call<TerminalOpened>("POST", `/api/hosts/${encodeURIComponent(name)}/open`, open),
  /** Opens a terminal inside Hub, logged in on that machine (`TerminalPane.svelte` shows it). */
  machineTerminal: (name: string, open: OpenTerminal) =>
    call<TerminalSession>("POST", `/api/hosts/${encodeURIComponent(name)}/terminals`, open),
  terminals: () => call<TerminalSession[]>("GET", "/api/terminals"),
  closeTerminal: (id: string) => call<void>("DELETE", `/api/terminals/${encodeURIComponent(id)}`),
  engineClis: () => call<EngineCli[]>("GET", "/api/engines/clis"),
  hosts: () => call<HostInfo[]>("GET", "/api/hosts"),
  addHost: (h: NewHost) => call<HostInfo>("POST", "/api/hosts", h),
  removeHost: (name: string) => call<void>("DELETE", `/api/hosts/${encodeURIComponent(name)}`),
  reconnectHost: (name: string) => call<void>("POST", `/api/hosts/${encodeURIComponent(name)}/reconnect`),
  /** Puts this machine's SSH key on a machine that refuses it, with its login password once. */
  letIn: (name: string, password: string) => call<void>("POST", `/api/hosts/${encodeURIComponent(name)}/key`, { password }),
  /** Sends a machine this one's version the way it takes updates, or installs hubd there. */
  updateHost: (name: string) => call<InstallAnswer>("POST", `/api/hosts/${encodeURIComponent(name)}/update`),
  /** Changes a machine added from the app (feature `hosts.settings`); `push_quiet_mins` 0 is no gap. */
  setHost: (name: string, settings: { push_quiet_mins?: number }) => call<HostInfo>("PATCH", `/api/hosts/${encodeURIComponent(name)}`, settings),
  installHost: (name: string, answer: InstallRequest = {}) =>
    call<InstallAnswer>("POST", `/api/hosts/${encodeURIComponent(name)}/install`, answer),
  /** The machines on this machine's tailnet, matched to the ones in Hub. */
  tailnet: () => call<Tailnet>("GET", "/api/tailnet"),
  /** Adds a machine and installs hubd there when it has none. */
  setUpHost: (h: NewHost) => call<SetUpAnswer>("POST", "/api/tailnet/setup", h),
  /** Machines on the home network that answer SSH; takes a few seconds. */
  lan: () => call<LanMachine[]>("GET", "/api/lan"),
  /** Installs Tailscale on a machine SSH reaches; the sign-in link shows on its row. */
  joinTailnet: (name: string, answer: InstallRequest = {}) =>
    call<InstallAnswer>("POST", `/api/hosts/${encodeURIComponent(name)}/tailnet`, answer),
  dismissTailnet: (name: string) => call<void>("POST", `/api/tailnet/${encodeURIComponent(name)}/dismiss`),
  pairCode: () => call<PairCode>("POST", "/api/pair"),
  /** A QR code for the phone page; lets Tailscale serve this machine on the tailnet first. */
  phoneLink: () => call<PhoneLink>("POST", "/api/phone"),
  devices: () => call<Device[]>("GET", "/api/devices"),
  revokeDevice: (id: string) => call<void>("DELETE", `/api/devices/${encodeURIComponent(id)}`),
  stopProcess: (pid: number, host?: string | null) =>
    call<void>("POST", `/api/host/processes/${pid}/stop${host ? `?host=${encodeURIComponent(host)}` : ""}`),
  pauseProcess: (pid: number) => call<void>("POST", `/api/host/processes/${pid}/pause`),
  resumeProcess: (pid: number) => call<void>("POST", `/api/host/processes/${pid}/resume`),
  workers: (host?: string | null) => call<Workers>("GET", `/api/workers${on(host)}`),
  /** The panic button, on every machine. */
  halt: () => call<Halt | null>("GET", "/api/halt"),
  pauseAll: () => call<Halt>("POST", "/api/halt/pause", {}),
  resumeAll: () => call<Resumed>("POST", "/api/halt/resume", {}),
  ownTools: (host?: string | null, fresh = false) => call<OwnTool[]>("GET", `/api/tools?fresh=${fresh}${at(host)}`),
  ownToolsWaiting: (host?: string | null) => call<{ folder: string; name: string }[]>("GET", `/api/tools/unapproved${on(host)}`),
  allowOwnTool: (folder: string, host?: string | null) => call<void>("POST", `/api/tools/allow${on(host)}`, { folder }),
  runToolAction: (ask: RunOwnToolAction, host?: string | null) => call<OwnToolRan>("POST", `/api/tools/action${on(host)}`, ask),
  draftHandoff: (id: string, folder: string) => call<HandoffBrief>("POST", `/api/threads/${id}/handoff/draft`, { folder }),
  handOff: (id: string, brief: HandoffBrief) => call<OwnToolRan>("POST", `/api/threads/${id}/handoff/tool`, brief),
  /** One press on the agent's handoff card: drafted, answered and handed over in the background. */
  handOffNow: (id: string, folder: string) => call<{ started: boolean }>("POST", `/api/threads/${id}/handoff/now`, { folder }),
  /** The newest run of your Workers in each thread that shows one. */
  workerRuns: () => call<Record<string, WorkerRun>>("GET", "/api/worker-runs"),
  workerControl: (ask: RunWorkerControl) => call<OwnToolRan>("POST", "/api/worker-runs/control", ask),
  shutdown: (id: string, everything: boolean, host?: string | null) => call<void>("POST", `/api/threads/${id}/shutdown${on(host)}`, { everything }),
  openTerminal: (id: string) => call<TerminalOpened>("POST", `/api/threads/${id}/terminal`),
  handBack: (id: string) => call<Thread>("POST", `/api/threads/${id}/handback`),
  services: (path: string, host?: string | null) =>
    call<ProjectServices>("GET", `/api/project/services?path=${encodeURIComponent(path)}${at(host)}`),
  startServices: (path: string, name?: string, host?: string | null) =>
    call<ServicesDone>("POST", `/api/project/services/start${host ? `?host=${encodeURIComponent(host)}` : ""}`, { path, name }),
  stopServices: (path: string, name?: string, host?: string | null) =>
    call<ServicesDone>("POST", `/api/project/services/stop${host ? `?host=${encodeURIComponent(host)}` : ""}`, { path, name }),
  serviceLog: (path: string, name: string, host?: string | null) =>
    call<{ text: string }>("GET", `/api/project/services/log?path=${encodeURIComponent(path)}&name=${encodeURIComponent(name)}${at(host)}`),
  saveServices: (path: string, host?: string | null) =>
    call<{ file: string }>("POST", `/api/project/services/save${host ? `?host=${encodeURIComponent(host)}` : ""}`, { path }),
};
