// The API calls for Customs, the GitHub tab and a thread's Git button (part of `api`, see api.ts).
import type { CustomsItem } from "./bindings/CustomsItem";
import type { PluginDiscover } from "./bindings/PluginDiscover";
import type { PluginSearch } from "./bindings/PluginSearch";
import type { PluginDetail } from "./bindings/PluginDetail";
import type { CustomsEdit } from "./bindings/CustomsEdit";
import type { CustomsImport } from "./bindings/CustomsImport";
import type { CustomsPrefs } from "./bindings/CustomsPrefs";
import type { CustomsRegistry } from "./bindings/CustomsRegistry";
import type { CustomsActivity } from "./bindings/CustomsActivity";
import type { Thread } from "./bindings/Thread";
import type { WorkerReady } from "./bindings/WorkerReady";
import type { PullList } from "./bindings/PullList";
import type { PullCard } from "./bindings/PullCard";
import type { PullDetail } from "./bindings/PullDetail";
import type { PullAction } from "./bindings/PullAction";
import type { IssueList } from "./bindings/IssueList";
import type { IssueDetail } from "./bindings/IssueDetail";
import type { IssueAction } from "./bindings/IssueAction";
import type { BranchList } from "./bindings/BranchList";
import type { GitState } from "./bindings/GitState";
import type { GitShip } from "./bindings/GitShip";
import type { GitShipped } from "./bindings/GitShipped";
import type { GitDraft } from "./bindings/GitDraft";
import type { AgentKey } from "./bindings/AgentKey";
import type { NewAgentKey } from "./bindings/NewAgentKey";
import type { CreatedAgentKey } from "./bindings/CreatedAgentKey";
import { call, at, on, p } from "./apiCall";

export const customsApi = {
  // Customs: what agents may build with.
  pluginsDiscover: (fresh = false) => call<PluginDiscover>("GET", `/api/plugins/discover${fresh ? "?fresh=true" : ""}`),
  pluginsSearch: (q: string) => call<PluginSearch>("GET", `/api/plugins/search?q=${encodeURIComponent(q)}`),
  pluginRepo: (name: string) => call<PluginDetail>("GET", `/api/plugins/repo?name=${encodeURIComponent(name)}`),
  /** Installs a Worker from GitHub into the projects folder and runs its ready check. */
  installWorker: (repo: string) => call<WorkerReady>("POST", "/api/plugins/install", { repo }),
  workerReady: (folder: string) => call<WorkerReady>("POST", "/api/plugins/ready", { folder }),
  customs: () => call<CustomsItem[]>("GET", "/api/customs"),
  editCustoms: (e: CustomsEdit) => call<CustomsItem>("PUT", "/api/customs", e),
  removeCustoms: (kind: string, name: string) =>
    call<void>("DELETE", `/api/customs?kind=${encodeURIComponent(kind)}&name=${encodeURIComponent(name)}`),
  importCustoms: (folder: string) => call<CustomsImport>("POST", "/api/customs/import", { folder }),
  reviewCustoms: (kind: string, name: string) => call<CustomsItem | null>("POST", "/api/customs/review", { kind, name }),
  customsPrefs: () => call<CustomsPrefs>("GET", "/api/customs/prefs"),
  setCustomsPrefs: (p: CustomsPrefs) => call<CustomsPrefs>("PUT", "/api/customs/prefs", p),
  customsRegistry: () => call<CustomsRegistry>("GET", "/api/customs/registry"),
  /** What happens at Customs: the last month of its log, or all of one item's, and the reviews running now. */
  customsActivity: (kind?: string, name?: string) =>
    call<CustomsActivity>("GET", kind && name ? `/api/customs/activity?kind=${encodeURIComponent(kind)}&name=${encodeURIComponent(name)}` : "/api/customs/activity"),
  agentKeys: () => call<AgentKey[]>("GET", "/api/agent-keys"),
  createAgentKey: (k: NewAgentKey) => call<CreatedAgentKey>("POST", "/api/agent-keys", k),
  revokeAgentKey: (id: string) => call<void>("DELETE", `/api/agent-keys/${id}`),
  deskFolder: () => call<{ folder: string }>("GET", "/api/desk"),
  /** The ids of every project's desk thread. */
  desks: () => call<{ ids: string[] }>("GET", "/api/desks"),
  /** A project's desk thread, made the first time. */
  openDesk: (folder: string, host?: string | null) => call<Thread>("POST", `/api/desks${on(host)}`, { folder }),
  // The GitHub tab, through the gh CLI of the machine the project is on.
  pulls: (path: string, state: string, host?: string | null) =>
    call<PullList>("GET", `/api/github/pulls?${p(path)}&state=${state}${at(host)}`),
  pull: (path: string, number: number, host?: string | null) =>
    call<PullDetail>("GET", `/api/github/pull?${p(path)}&number=${number}${at(host)}`),
  /** The small card for a pull request named in a chat; `repo` is `owner/name` when it is not the project's own. */
  pullCard: (path: string, repo: string, number: number, host?: string | null) =>
    call<PullCard>("GET", `/api/github/card?${p(path)}&repo=${encodeURIComponent(repo)}&number=${number}${at(host)}`),
  pullDiff: (path: string, number: number, host?: string | null) =>
    call<{ text: string }>("GET", `/api/github/diff?${p(path)}&number=${number}${at(host)}`),
  pullAction: (a: PullAction, host?: string | null) => call<void>("POST", `/api/github/action${on(host)}`, a),
  issues: (path: string, state: string, host?: string | null) =>
    call<IssueList>("GET", `/api/github/issues?${p(path)}&state=${state}${at(host)}`),
  issue: (path: string, number: number, host?: string | null) =>
    call<IssueDetail>("GET", `/api/github/issue?${p(path)}&number=${number}${at(host)}`),
  issueAction: (a: IssueAction, host?: string | null) => call<number>("POST", `/api/github/issue${on(host)}`, a),
  branchList: (path: string, host?: string | null) => call<BranchList>("GET", `/api/github/branches?${p(path)}${at(host)}`),
  deleteBranches: (path: string, names: string[], local: boolean, remote: boolean, host?: string | null) =>
    call<string[]>("DELETE", `/api/github/branches${on(host)}`, { path, names, local, remote }),
  /** The pull request each thread watches, by thread. */
  watches: () => call<Record<string, number>>("GET", "/api/github/watches"),
  watch: (id: string, number?: number) => call<number>("POST", `/api/threads/${id}/watch`, { number }),
  unwatch: (id: string) => call<void>("DELETE", `/api/threads/${id}/watch`),
  // The thread's Git button.
  gitState: (id: string) => call<GitState>("GET", `/api/threads/${id}/gitstate`),
  localBranches: (id: string) => call<string[]>("GET", `/api/threads/${id}/branches`),
  switchBranch: (id: string, branch: string, create = false) => call<void>("POST", `/api/threads/${id}/switch`, { branch, create }),
  ship: (id: string, s: Omit<GitShip, "cwd">) => call<GitShipped>("POST", `/api/threads/${id}/ship`, { cwd: "", ...s }),
  gitDraft: (id: string) => call<GitDraft>("POST", `/api/threads/${id}/gitdraft`),
};
