// HubState: the single app state (threads, projects, view, settings) and the actions on it.
import { quiet, quietly } from "./quiet";
import { ThreadLists } from "./threadLists.svelte";
import { GUARD, isGuardChat, isGuardErrand, isSystem } from "./system";
import { api, type NewProjectMore, type StreamItem } from "./api";
import type { Attachment } from "./bindings/Attachment";
import type { ThreadReach } from "./bindings/ThreadReach";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import type { Aside } from "./bindings/Aside";
import type { Halt } from "./bindings/Halt";
import type { OpenOnMachine } from "./bindings/OpenOnMachine";
import type { InboxIdea } from "./bindings/InboxIdea";
import type { Event } from "./bindings/Event";
import type { SettingValue } from "./bindings/SettingValue";
import type { HostSnapshot } from "./bindings/HostSnapshot";
import type { NewProject } from "./bindings/NewProject";
import type { Project } from "./bindings/Project";
import { prefs, setPref, type Held } from "./prefs.svelte";
import { isChatThread } from "./format";
import { runNotices, sortOf } from "./notices";
import { left, newSince, visited } from "./visits";
import type { Thread } from "./bindings/Thread";
import type { GuardPrefs } from "./bindings/GuardPrefs";
import type { AccountUsage } from "./bindings/AccountUsage";
import type { HostInfo } from "./bindings/HostInfo";
import type { ThreadPr } from "./bindings/ThreadPr";
import type { WorkerRun } from "./bindings/WorkerRun";
import type { FixCard } from "./bindings/FixCard";
import type { ThreadPatch } from "./bindings/ThreadPatch";
import { ThreadActions } from "./threadActions.svelte";
import type { PromptLink } from "./bindings/PromptLink";
import { poll } from "./poll";
import { Stream } from "./stream.svelte";
import { Machines } from "./machines.svelte";
import { asksYou, chatMark, isSnoozed, needsYou, recentFolders, rememberFolder, unread, wakeLabel } from "./threadFlags";
import type { GoTarget, ProjectTab, Room, Route, SettingsSection, View, Viewing, World } from "./viewTypes";
import { roomOf } from "./viewTypes";
import { StandIns, isStandIn } from "./standIns.svelte";
import { toDrop } from "./logCache";
import { enterFocus, focusDesk, goIn, inFocus, leaveFocus, projectName } from "./focus";
import { enterRoom, setWorld, switchRoom, switchWorld, syncWorld, threadHere } from "./worlds";
import { draftFolder, fromLink, fromPopped, goTo, stageIn, startDraft } from "./launch";
import { addProject, askLibrary, learn, makeProjectOf, openProject, removeProject } from "./projectNav";

// What other files import from here keeps working: the types and the pure thread checks live in
// `viewTypes.ts` and `threadFlags.ts`, and are passed on.
export type { GoTarget, ProjectTab, Room, Route, SettingsSection, View, Viewing, World } from "./viewTypes";
export { roomOf } from "./viewTypes";
export { asksYou, chatMark, isSnoozed, needsYou, recentFolders, unread, wakeLabel } from "./threadFlags";


export { isStandIn };



export class HubState {
  threads = $state<Thread[]>([]);
  /** Folders added as projects; folders with threads are projects without being here. */
  projects = $state<Project[]>([]);
  newProjectOpen = $state(false);
  /** A quick (Scratch) thread being made a project: the dialog starts from its title and folder. */
  newProjectFrom = $state<{ name: string; about: string; from: string; host: string | null } | null>(null);
  /** The project just made, until its page's first-steps lines are dismissed or the first thread is sent. */
  firstRun = $state<{ path: string; host: string | null; thread: string | null } | null>(null);
  /** The idea inbox, newest first, and whether its box is open (Ctrl I). */
  ideas = $state<InboxIdea[]>([]);
  /** The latest note of the judge per thread: what it did with a message, or its answer (`judge.rs`). */
  asides = $state<Record<string, Aside>>({});
  /** Open errors that came in since the Problems page was last looked at (the sidebar's count). */
  problemsNew = $state<number[]>([]);
  /** Problems that need the user (asks first, came back, urgent with nobody on it, or unsorted while
   *  Vakten is off), on this machine and every online one, as `machine:id` (this machine is "").
   *  The same set the Problems page's "Needs you" tab lists: one problem is one, however the page folds rows. */
  problemsNeedYou = $state<string[]>([]);
  /** What Vakten told the user lately, newest first. */
  problemNews = $state<{ id: number | null; text: string; ts: number }[]>([]);
  /** Goes up with every problem seen or changed, so an open Problems page reads the list again. */
  problemsTick = $state(0);
  /** A problem to open when the Problems page shows, from a failure's card in a thread. */
  problemPick = $state<{ id: number; host: string | null } | null>(null);
  ideaOpen = $state(false);
  /** Events of threads that have been opened, keyed by thread id. Raw rather than deep state:
   *  a long thread has tens of thousands of events, and folding them through proxies on every
   *  streamed line kept the window busy. Replaced whole when events arrive, at most once a frame. */
  events = $state.raw<Record<string, Event[]>>({});
  private stream = new Stream(this);

  /** The machines, the updater and the subscriptions' limits live in `machines.svelte.ts`; the
   *  fields and calls below pass on to it, so `hub.update`, `hub.plan` and the rest read as before. */
  private machines = new Machines(this);
  get tailnet() { return this.machines.tailnet; }
  set tailnet(v) { this.machines.tailnet = v; }
  get update() { return this.machines.update; }
  set update(v) { this.machines.update = v; }
  get installing() { return this.machines.installing; }
  set installing(v) { this.machines.installing = v; }
  get plan() { return this.machines.plan; }
  set plan(v) { this.machines.plan = v; }
  get others() { return this.machines.others; }
  set others(v) { this.machines.others = v; }
  get notes() { return this.machines.notes; }
  set notes(v) { this.machines.notes = v; }
  get installAsked() { return this.machines.installAsked; }
  set installAsked(v) { this.machines.installAsked = v; }
  get tour() { return this.machines.tour; }
  set tour(v) { this.machines.tour = v; }
  get machineAsk() { return this.machines.machineAsk; }
  set machineAsk(v) { this.machines.machineAsk = v; }
  get subscriptions() { return this.machines.subscriptions; }
  get freshNotes() { return this.machines.freshNotes; }
  get updateReady() { return this.machines.updateReady; }
  get newMachines() { return this.machines.newMachines; }
  setUsage(u: Parameters<Machines["setUsage"]>[0]) { this.machines.setUsage(u); }
  installUpdate() { return this.machines.installUpdate(); }
  refreshProblemsNeedYou() { return this.machines.refreshProblemsNeedYou(); }
  refreshTailnet() { return this.machines.refreshTailnet(); }
  setUpMachine(target: { name: string; ssh: string }, password?: string) { return this.machines.setUpMachine(target, password); }
  dismissMachine(name: string) { return this.machines.dismissMachine(name); }
  onUpdate(status: UpdateStatus) { this.machines.onUpdate(status); }
  host = $state<HostSnapshot | null>(null);
  /** Machines threads can run on, this one first. */
  hosts = $state<HostInfo[]>([]);
  /** Only threads on this machine show in the sidebar, or all when null. */
  hostFilter = $state<string | null>(null);
  /** Every agent paused from the panic button, on this machine and the others; null while they run. */
  halt = $state<Halt | null>(null);
  /** Where the name menu (account, Settings, What's new) is open: from the avatar on the rail, from
   *  the usage strip at the foot of the sidebar, or not at all. One menu, two ways in. */
  meMenu = $state<"rail" | "foot" | null>(null);
  guard = $state<GuardPrefs | null>(null);
  connected = $state(false);
  /** The thread list has come from the core at least once. */
  loaded = $state(false);
  view = $state<View>("home");
  selected = $state<string | null>(null);
  /** Where "New" starts in each thread you have open: when you last looked at it. Set when you come
   *  back to it and dropped when you go, so the break stays while you read and is gone next time. */
  since = $state<Record<string, number>>({});
  /** Folder of the project page that is open. */
  project = $state<string | null>(null);
  /** The machine that folder is on, or null for this one. */
  projectHost = $state<string | null>(null);
  /** The machine whose page is open, by name. */
  machine = $state<string | null>(null);
  /** The terminal inside Hub a machine's page shows (`TerminalPane.svelte`); null shows none. */
  machineTerminal = $state<string | null>(null);
  settingsSection = $state<SettingsSection>("account");
  settingsOpen = $state(false);
  /** A setting to point at once Settings has opened, by the name it shows. */
  settingsSpot = $state<string | null>(null);
  /** The API connection whose page Settings shows, or null for the list. */
  apiOpen = $state<string | null>(null);
  /** The key open on its own page in Settings > Key vault, by name. */
  keyOpen = $state<string | null>(null);
  projectTab = $state<ProjectTab>("overview");
  /** The doc open on the docs tab, a path in the project, with `#heading` when it was linked so. */
  projectDoc = $state<string | null>(null);
  /** The thing open big on the library tab, by its id; null shows the list. */
  projectItem = $state<string | null>(null);
  /** The note open in the library, a path or a name, with `#heading` when it was linked so. */
  libraryDoc = $state<string | null>(null);
  /** The repository the Plugins page shows, as `owner/name`; null shows Discover. */
  pluginRepo = $state<string | null>(null);
  sidebarOpen = $state(true);
  panelOpen = $state(true);
  /** A command to unfold under Running in the side column, by tool call id. */
  terminalFocus = $state<string | null>(null);
  /** Counts showCommand calls, so the same command asked for twice unfolds it again. */
  terminalAsked = $state(0);
  /** Threads taken over in a terminal inside Hub, by thread id: the terminal's id (`pty.rs`). */
  takeovers = $state<Record<string, string>>({});
  /** Counts showTakeover calls, so the side column scrolls to the terminal each time. */
  takeoverAsked = $state(0);
  /** Something the agent showed in a thread, open big in the right panel (`Viewer`). */
  viewing = $state<Viewing | null>(null);
  error = $state<string | null>(null);
  /** A button on the error toast that fixes what it says, e.g. "Sign in". */
  errorAction = $state<{ label: string; run: () => unknown } | null>(null);
  /** A short note that something worked, such as a copy. */
  notice = $state<string | null>(null);
  /** Branch of each thread on this machine, for the activity list. */
  branches = $state<Record<string, string>>({});
  /** The pull request of each thread's branch, and when threads stopped by a plan limit carry on. */
  prs = $state<Record<string, ThreadPr>>({});
  /** The newest run of your Workers in each thread that shows one (`worker_runs.rs`). */
  workerRuns = $state<Record<string, WorkerRun>>({});
  /** Fix threads from Problems on every online machine, by thread id: where each stands. */
  fixCards = $state<Record<string, FixCard>>({});
  waits = $state<Record<string, number>>({});
  /** The pull request each thread has Hub watch for it. */
  watches = $state<Record<string, number>>({});
  /** The project's GitHub tab: what it lists and which pull request or issue is open. */
  github = $state<{ mode: "pulls" | "issues" | "branches"; pull: number | null; issue: number | null }>({ mode: "pulls", pull: null, issue: null });
  /** A pull request opened from a thread, big in the right panel. */
  pull = $state<{ thread: string; path: string; host: string | null; number: number } | null>(null);
  /** The clock the lists read, so snoozes wake and times move on. */
  now = $state(Date.now());
  /** Threads picked in the activity list with Ctrl or Shift click, for doing one thing to all. */
  picked = $state<string[]>([]);
  /** The last thing done from the activity list, which Ctrl Z takes back for a few seconds. */
  undo = $state<{ label: string; run: () => void; at: number } | null>(null);
  /** A first message waiting on Home, e.g. a handoff from a long thread. */
  /** `from`: the machine `cwd` is on, when the draft should run on another (`host`); Home then
   * finds the same repository there. */
  draft = $state<{ cwd: string; text: string; host: string | null; attachments?: Attachment[]; everything?: boolean; from?: string | null } | null>(null);
  /** The page each room had open last, so switching rooms takes you back to it. */
  rooms = $state<Record<Room, Route>>({ work: { view: "home" }, control: { view: "overview" } });
  /** The project Work is narrowed to, after switching over from that project in Control. `quick`
   *  is every quick (Scratch) thread on every machine, not one folder. */
  workScope = $state<{ cwd: string; host: string | null; name: string; quick?: boolean } | null>(null);
  /** Attachments shared into a thread from elsewhere, waiting for its message box. */
  staged = $state.raw<Record<string, Attachment[]>>({});

  route = $derived.by((): Route => {
    if (this.view === "thread" && this.selected) return { view: "thread", thread: this.selected };
    if (this.view === "project" && this.project)
      return {
        view: "project",
        project: this.project,
        host: this.projectHost,
        tab: this.projectTab,
        ...(this.projectTab === "docs" ? { doc: this.projectDoc } : {}),
        ...(this.projectTab === "library" ? { item: this.projectItem } : {}),
      };
    if (this.view === "library") return { view: "library", doc: this.libraryDoc };
    if (this.view === "plugins") return { view: "plugins", repo: this.pluginRepo };
    if (this.view === "machine" && this.machine) return { view: "machine", machine: this.machine };
    if (this.view === "thread" || this.view === "project" || this.view === "machine") return { view: "home" };
    return { view: this.view };
  });
  current = $derived(this.threads.find((t) => t.id === this.selected) ?? null);
  /** The side jobs thread `id` started, oldest first. */
  childrenOf(id: string): Thread[] {
    return this.threads.filter((t) => t.parent === id && !t.archived).sort((a, b) => a.created_at - b.created_at);
  }
  /** The name of this machine, which threads without a host run on. */
  localHost = $derived(this.hosts.find((h) => h.local)?.name ?? "");
  /** The ids of every project's desk (`desk.rs`): the one thread per project you steer the
   *  others from. Lists leave desks out unless one needs you; the focus bar leads to them. */
  desks = $state<string[]>([]);
  isDesk = (t: Thread) => this.desks.includes(t.id);
  /** The lists of threads (`threadLists.svelte.ts`); the getters below keep `hub.inbox` and the rest. */
  lists = new ThreadLists(this);
  get everything() { return this.lists.everything; }
  get systemThreads() { return this.lists.systemThreads; }
  get chats() { return this.lists.chats; }
  get active() { return this.lists.active; }
  get withChats() { return this.lists.withChats; }
  get archived() { return this.lists.archived; }
  get hubAsks() { return this.lists.hubAsks; }
  get guardAsks() { return this.lists.guardAsks; }
  get guardThreads() { return this.lists.guardThreads; }
  get fromHub() { return this.lists.fromHub; }
  get waiting() { return this.lists.waiting; }
  get asking() { return this.lists.asking; }
  get running() { return this.lists.running; }
  get pinned() { return this.lists.pinned; }
  get snoozed() { return this.lists.snoozed; }
  get working() { return this.lists.working; }
  get inbox() { return this.lists.inbox; }
  get settled() { return this.lists.settled; }
  get strays() { return this.lists.strays; }
  get quota() { return this.lists.quota; }
  private renamed = new Set<string>();
  /** The core still starts the guard's chat as "Vakten"; the app, once, calls it Guard. A title
   *  you changed yourself stays. */
  nameGuard() {
    const t = this.guardChat;
    if (!t || t.title !== "Vakten" || this.popped || this.renamed.has(t.id)) return;
    this.renamed.add(t.id);
    api.patchThread(t.id, { title: GUARD }).then((u) => this.upsert(u)).catch(() => this.renamed.delete(t.id));
  }
  /** The Guard's own chat on this machine, if it has one. */
  guardChat = $derived(this.threads.find((t) => !t.archived && !t.host && isGuardChat(t)) ?? null);
  /** The core sends a stuck, asking or failed fix thread of Guard's back to Guard (`guard.reports`):
   *  then only a live permission question of one still waits on you. */
  guardReports = $state(false);
  /** Guard takes this thread's question or failure, not you. */
  guardHandles(t: Thread): boolean {
    return this.guardReports && isGuardErrand(t) && t.status !== "waiting_on_you";
  }
  /** The subscription picked in the name menu, shown next to your name. */
  get subscription(): AccountUsage {
    const all = this.subscriptions;
    return all.find((s) => s.engine === prefs.menuAccount) ?? all[0];
  }

  /** The quota strip level the person said OK to, for every chat and thread. */
  quotaAcked = $state<string | null>(typeof localStorage !== "undefined" ? localStorage.getItem("hub.quotaAcked") : null);
  ackQuota(key: string) {
    this.quotaAcked = key;
    try { localStorage.setItem("hub.quotaAcked", key); } catch {}
  }

  /** Memory nearly full, worth a word on Home. */
  strained = $derived(this.host ? this.host.mem_used / Math.max(this.host.mem_total, 1) >= 0.9 : false);

  start() {
    return this.stream.start();
  }

  resync() {
    return this.stream.resync();
  }

  refreshHost() {
    return this.stream.refreshHost();
  }

  apply(msg: StreamItem) {
    this.stream.apply(msg);
  }

  catchUp(id: string) {
    return this.stream.catchUp(id);
  }


  /** Tells you when a thread needs you or has finished, if you are not looking at Hub. In focus
   *  mode, what other projects would say waits until you leave it. */
  announce(before: Thread, after: Thread) {
    // The main window tells you; a popped-out viewer would say it twice.
    if (before.status === after.status || this.popped) return;
    if (!this.inFocus(after)) {
      const what = after.status === "waiting_on_you" ? "waiting" : after.status === "error" ? "failed" : before.status === "running" && after.status === "idle" ? "done" : null;
      if (what) setPref("held", [...prefs.held.filter((h) => h.id !== after.id), { id: after.id, title: after.title, what, at: Date.now() }]);
      return;
    }
    const sort = sortOf(before, after);
    if (!sort) return;
    // Hub's own threads finish turns all day, and a fix thread of the Guard's finishes quietly (the
    // Problems page says how it went); only one that needs you or fails is worth a word.
    if (sort === "done" && (isSystem(after) || isGuardErrand(after))) return;
    const id = after.id;
    runNotices(after, sort, () => this.threads.find((t) => t.id === id)?.status === after.status);
  }

  /** Opens or folds the thread's commands, under Running in the side column (Ctrl §). */
  toggleTerminal() {
    this.terminalFocus = null;
    const shut = prefs.panelShut.includes("run");
    if (this.panelOpen && !shut) setPref("panelShut", [...prefs.panelShut, "run"]);
    else this.showCommand(null);
  }

  /** Opens the side column at the thread's takeover terminal, unfolded. */
  showTakeover(thread: string, terminal: string) {
    this.takeovers = { ...this.takeovers, [thread]: terminal };
    this.panelOpen = true;
    if (prefs.panelShut.includes("takeover")) setPref("panelShut", prefs.panelShut.filter((k) => k !== "takeover"));
    this.takeoverAsked++;
  }

  /** Forgets the thread's takeover terminal and ends what still runs in it. */
  async closeTakeover(thread: string) {
    const id = this.takeovers[thread];
    if (!id) return;
    const { [thread]: _, ...rest } = this.takeovers;
    this.takeovers = rest;
    await quiet(api.closeTerminal(id), "close terminal");
  }

  /** Opens the side column at a command the agent ran, its output unfolded. */
  showCommand(id: string | null) {
    this.panelOpen = true;
    if (prefs.panelShut.includes("run")) setPref("panelShut", prefs.panelShut.filter((k) => k !== "run"));
    this.terminalFocus = id;
    this.terminalAsked++;
  }

  forget(id: string) {
    // A thread being moved leaves the machine it was on a moment before it shows on the other
    // one, with the same id. That "gone" is not a delete: its row and its chat stay as they are.
    if (this.holding.has(id) || this.unheard.has(id)) return;
    this.threads = this.threads.filter((t) => t.id !== id);
    const { [id]: _, ...rest } = this.events;
    this.events = rest;
    delete this.stream.arrived[id];
    delete this.asides[id];
    if (this.selected === id) this.goHome();
  }

  /** Threads on their way to another machine, as they were when they set off. The list and the
   *  chat show these until the move is done, so nothing on screen jumps while it runs. */
  holding = new Map<string, Thread>();
  /** Threads that just moved: the old machine's "gone" can reach us a moment after the answer. */
  unheard = new Set<string>();

  /** The list from the core, with a thread being moved kept as it was. */
  held(list: Thread[]): Thread[] {
    if (!this.holding.size) return list;
    return [...list.filter((t) => !this.holding.has(t.id)), ...this.holding.values()];
  }

  /** Thread actions live in threadActions.svelte.ts; these are the same calls on `hub`. */
  private actions = new ThreadActions(this);
  moveThread(t: Thread, to: string, stop: boolean) { return this.actions.moveThread(t, to, stop); }
  rename(id: string, title: string) { return this.actions.rename(id, title); }
  setArchived(id: string, archived: boolean) { return this.actions.setArchived(id, archived); }
  mark(ids: string[], patch: ThreadPatch, label: string) { return this.actions.mark(ids, patch, label); }
  pin(ids: string[], pinned: boolean) { return this.actions.pin(ids, pinned); }
  pinAt(id: string, above: number | null, below: number | null) { return this.actions.pinAt(id, above, below); }
  snooze(ids: string[], until: number) { return this.actions.snooze(ids, until); }
  markUnread(id: string) { return this.actions.markUnread(id); }
  setAutoSettle(id: string, auto: boolean) { return this.actions.setAutoSettle(id, auto); }
  retitle(id: string) { return this.actions.retitle(id); }
  settleMany(ids: string[], settled: boolean) { return this.actions.settleMany(ids, settled); }
  settle(id: string, settled: boolean) { return this.actions.settle(id, settled); }
  markSeen(id: string) { return this.actions.markSeen(id); }
  remove(id: string) { return this.actions.remove(id); }
  upsert(thread: Thread) { return this.actions.upsert(thread); }

  private branchTimer: ReturnType<typeof setTimeout> | undefined;

  /** Fetches branches soon, once for a burst of changes. */
  refreshBranches() {
    clearTimeout(this.branchTimer);
    this.branchTimer = setTimeout(async () => {
      // This machine's threads, and those of every other machine that is online.
      const others = this.hosts.filter((h) => !h.local && h.state === "online").map((h) => h.name);
      const hosts = [undefined, ...others];
      const found = await Promise.all(hosts.map((h) => quiet(api.branches(h), "branches", null)));
      // A machine that did not answer keeps the branches it had, so its rows do not shrink and grow.
      const kept = (h: string | undefined) =>
        Object.fromEntries(Object.entries(this.branches).filter(([id]) => (this.threads.find((t) => t.id === id)?.host ?? undefined) === h));
      this.branches = Object.assign({}, ...found.map((f, i) => f ?? kept(hosts[i])));
      // Pull requests are looked up by the core every few minutes; waits change with errors.
      quiet(api.prs().then((p) => (this.prs = p)), "prs");
      quiet(api.workerRuns().then((r) => (this.workerRuns = r)), "worker runs");
      Promise.all(hosts.map((h) => api.fixCards(h).catch(() => ({}))))
        .then((all) => (this.fixCards = Object.assign({}, ...all)))
        .catch(quietly("fix cards"));
      quiet(api.waits().then((w) => (this.waits = w)), "waits");
      quiet(api.watches().then((w) => (this.watches = w)), "watches");
    }, 300);
  }

  /** The list as fetched, with what the stream brought meanwhile kept where it is newer. */
  newest(fetched: Thread[]): Thread[] {
    const known = new Map(this.threads.map((t) => [t.id, t]));
    return fetched.map((t) => {
      const have = known.get(t.id);
      return have && have.updated_at > t.updated_at ? have : t;
    });
  }

  /** Goes to a place back or forward found. */
  show(r: Route) {
    if (r.view === "thread") return this.open(r.thread);
    this.view = r.view;
    this.selected = null;
    if (r.view === "project") {
      this.project = r.project;
      this.projectHost = r.host;
      this.projectTab = r.tab;
      if (r.doc !== undefined) this.projectDoc = r.doc;
      this.projectItem = r.item ?? null;
    }
    if (r.view === "library") this.libraryDoc = r.doc;
    if (r.view === "plugins") this.pluginRepo = r.repo;
    if (r.view === "machine") this.machine = r.machine;
  }

  /** Opens the Plugins page: one repository's own page by `owner/name`, or Discover. */
  openPlugins(repo: string | null = null) {
    this.view = "plugins";
    this.selected = null;
    this.pluginRepo = repo;
  }

  /** Opens a terminal inside Hub on a machine, a shell, a root shell or an engine's CLI there,
   * and shows it on the machine's page. */
  async openTerminal(name: string, open: OpenOnMachine) {
    try {
      const t = await api.machineTerminal(name, { ...open, thread: null, cols: 120, rows: 32 });
      this.openMachine(name);
      this.machineTerminal = t.id;
    } catch (e) {
      this.fail(e);
    }
  }

  /** Opens a machine's own page, by name. */
  openMachine(name: string) {
    this.view = "machine";
    this.machine = name;
    this.selected = null;
  }

  /** Opens a note in the library, by path or by name: a stack chip's "Node.js" opens tech/Node.js.md. */
  openLibrary(doc: string | null = null) {
    this.view = "library";
    this.selected = null;
    this.libraryDoc = doc;
  }

  async learn(note: string) { return learn(this, note); }

  async askLibrary(question: string) { return askLibrary(this, question); }

  open(id: string) {
    const here = this.view === "thread" && this.selected === id;
    const t = this.threads.find((t) => t.id === id);
    // Before `seen_at` is moved up to now: this is how far back "New" reaches.
    if (!here) this.arrive(id, t?.seen_at);
    this.view = "thread";
    this.selected = id;
    visited(id);
    if (t && unread(t)) this.markSeen(id);
    this.follow(id);
    this.dropOldLogs(id);
  }

  /** Coming back to a thread: "New" starts where you stopped looking. */
  arrive(id: string, seenAt: number | null | undefined) {
    const at = newSince(id, seenAt);
    if (at != null) this.since = { ...this.since, [id]: at };
  }

  /** Looking away from a thread (another thread, another page, the window hidden): what is new
   *  now is seen, and the next time starts from here. */
  leave(id: string) {
    left(id);
    if (id in this.since) {
      const { [id]: _, ...rest } = this.since;
      this.since = rest;
    }
  }

  /** Loads a thread's events and keeps them coming. */
  follow(id: string) {
    this.opened = this.opened.filter((o) => o !== id).concat(id);
    if (!this.events[id]) {
      this.events = { ...this.events, [id]: [] };
      this.catchUp(id).catch((e) => this.fail(e));
    }
  }

  /** Thread ids in the order their logs were last asked for, oldest first (see `logCache.ts`). */
  private opened: string[] = [];

  /** Lets go of the logs of threads long out of sight (only when a thread is opened, so a panel
   *  that follows one cannot make another reload in a loop), so memory and reconnecting do not grow with
   *  every thread opened in a long session. One is loaded again when it is opened. */
  private dropOldLogs(now: string) {
    const must = new Set([now, ...this.desks]);
    if (this.selected) must.add(this.selected);
    if (this.viewing) must.add(this.viewing.thread);
    for (const t of this.threads) if (t.status === "running" || t.status === "waiting_on_you") must.add(t.id);
    const held = Object.keys(this.events);
    for (const id of held) if (isStandIn(id) || this.holding.has(id)) must.add(id);
    const drop = new Set(toDrop(held, this.opened, must));
    if (!drop.size) return;
    this.events = Object.fromEntries(Object.entries(this.events).filter(([id]) => !drop.has(id)));
    this.opened = this.opened.filter((id) => !drop.has(id));
    for (const id of drop) delete this.stream.arrived[id];
  }

  /** Opens the thread that has waited on you longest, after the one you are in. */
  nextWaiting() {
    // What waits on you in Dev is not reached from Chat; the rail's world button says it waits.
    if (this.world === "chat") return false;
    const queue = [...this.waiting].sort((a, b) => a.updated_at - b.updated_at);
    if (!queue.length) return false;
    const at = queue.findIndex((t) => t.id === this.selected);
    this.open(queue[(at + 1) % queue.length].id);
    return true;
  }

  async addProject(p: NewProject & NewProjectMore, host: string | null = null): Promise<{ notes: string[]; path: string; thread: string | null }> { return addProject(this, p, host); }

  makeProjectOf(t: { title: string; cwd: string; host?: string | null }) { return makeProjectOf(this, t); }

  async removeProject(path: string) { return removeProject(this, path); }

  openProject(cwd: string, host: string | null = null) { return openProject(this, cwd, host); }

  startDraft(cwd: string, text: string, host: string | null = null, attachments?: Attachment[], everything?: boolean, from?: string | null) { return startDraft(this, cwd, text, host, attachments, everything, from); }

  stageIn(id: string, attachments: Attachment[]) { return stageIn(this, id, attachments); }

  fromLink(link: PromptLink) { return fromLink(this, link); }

  draftFolder(): string { return draftFolder(this); }

  /** This window is a viewer popped out of the main one: what it starts happens over there. */
  popped = false;

  fromPopped(focus: () => void) { return fromPopped(this, focus); }

  goTo(t: GoTarget) { return goTo(this, t); }

  /** Settings open as a window over whatever is showing; going anywhere closes them. */
  openSettings(section: SettingsSection = this.settingsSection, spot: string | null = null) {
    this.settingsSection = section;
    this.settingsSpot = spot;
    this.settingsOpen = true;
  }

  /** Which machine a thread runs on. */
  hostOf(t: Thread): string {
    return t.host ?? this.localHost;
  }

  hostInfo(name: string): HostInfo | undefined {
    return this.hosts.find((h) => h.name === name);
  }

  goHome() {
    // Chat has no Home: leaving a chat (deleting it, archiving it) lands on New chat, still in Chat.
    if (this.world === "chat") return this.goNew();
    this.view = "home";
    this.selected = null;
  }

  /** Opens New thread: the message box on its own page. Home is the board of what is going on. */
  goNew() {
    this.view = "new";
    this.selected = null;
  }

  room = $derived(roomOf(this.view));

  /** The world on screen: Chat only while a chat or New chat is. */
  world = $state<World>(prefs.world);
  worldRoute: Record<World, Route | null> = { dev: null, chat: null };

  syncWorld() { return syncWorld(this); }

  setWorld(to: World) { return setWorld(this, to); }

  switchWorld(to: World = this.world === "dev" ? "chat" : "dev") { return switchWorld(this, to); }

  /** Whether a thread belongs to the world on screen. Everything that offers other threads (a
   *  mention, a share, search) offers only these. */
  inWorld = (t: Thread): boolean => isChatThread(t) === (this.world === "chat");

  inFocus(t: Thread): boolean { return inFocus(this, t); }

  /** What focus mode held back, shown once you leave it until you close it. */
  released = $state<{ name: string; since: number; until: number; held: Held[] } | null>(null);

  projectName(cwd: string, host: string | null = null): string { return projectName(this, cwd, host); }

  enterFocus(cwd: string, host: string | null = null) { return enterFocus(this, cwd, host); }

  async goIn(cwd: string, host: string | null = null) { return goIn(this, cwd, host); }

  get focusDesk(): Thread | null { return focusDesk(this); }

  leaveFocus() { return leaveFocus(this); }

  switchRoom(to: Room = this.room === "work" ? "control" : "work") { return switchRoom(this, to); }

  threadHere(id: string): boolean { return threadHere(this, id); }

  enterRoom(to: Room) { return enterRoom(this, to); }

  /** Stand-ins for threads the core is still creating (`standIns.svelte.ts`); the calls below pass on to it. */
  private standIns = new StandIns(this);

  showStarting(s: { cwd: string; message: string; attachments?: Attachment[]; engine?: string; host?: string | null }): string {
    return this.standIns.show(s);
  }

  /** The thread a stand-in became; null if it did not start. Any other id is itself. */
  realId(id: string): Promise<string | null> {
    return this.standIns.realId(id);
  }

  async newThread(
    cwd: string,
    message: string,
    attachments: Attachment[] = [],
    settings: Record<string, SettingValue> = {},
    original?: string,
    host?: string,
    attempts?: Record<string, SettingValue>[],
    engine?: string,
    /** False starts it in the background, staying where you are. */
    open = true,
    /** The id of an engine started while the message was typed (`warm.ts`). */
    warm?: string,
    /** The stand-in already on screen for it (`showStarting`); one opens here otherwise. */
    shown?: string,
    /** Starts it in design mode: a design document before any code (`design.rs`). */
    design?: boolean,
    /** More folders and connections off for this thread (`thread.reach`). */
    reach?: ThreadReach,
    /** Runs it on Anthropic's or OpenAI's servers (`cloud.svelte.ts`), not on a machine. */
    cloud?: boolean,
  ): Promise<Thread> {
    // The wall again, at the one place every new thread passes: a chat starts only from Chat, in a
    // chats folder, and Dev's threads never do. A stand-in is dropped if the caller made one.
    if (isChatThread({ cwd }) !== (this.world === "chat")) {
      const e = new Error(this.world === "chat" ? "A chat starts in Chat, not in a project" : "A project's thread cannot start in the chats folder");
      if (shown) this.failStandIn(shown, cwd, message, attachments, host);
      this.fail(e);
      throw e;
    }
    const stand = shown ?? (open ? this.showStarting({ cwd, message, attachments, engine, host }) : undefined);
    try {
      const remote = host && host !== this.localHost ? host : undefined;
      const body = { cwd, message, attachments, settings, original, attempts, engine, warm, design, reach };
      const thread = await (cloud ? api.createCloudThread(body, remote) : api.createThread(body, remote));
      if (!this.threads.some((t) => t.id === thread.id)) this.threads.unshift(thread);
      if (stand) await this.standIns.swap(stand, thread.id);
      else if (open) this.open(thread.id);
      if (!remote) rememberFolder(cwd);
      return thread;
    } catch (e) {
      if (stand) this.failStandIn(stand, cwd, message, attachments, host);
      this.fail(e);
      throw e;
    }
  }

  failStandIn(stand: string, cwd: string, message: string, attachments: Attachment[] = [], host?: string | null) {
    this.standIns.fail(stand, cwd, message, attachments, host);
  }

  send(id: string, text: string, attachments: Attachment[] = [], original?: string, steer = false) {
    return this.standIns.send(id, text, attachments, original, steer);
  }

  setSetting(id: string, configId: string, value: SettingValue) {
    return this.standIns.setSetting(id, configId, value);
  }

  tell(text: string) {
    this.standIns.tell(text);
  }

  /** The panic button: stops every agent's turn on every machine; nothing starts until resumed. */
  halting = $state(false);
  async pauseAll() {
    if (this.halting) return;
    this.halting = true;
    try {
      this.halt = await api.pauseAll();
    } catch (e) {
      this.fail(e);
    } finally {
      this.halting = false;
    }
  }

  /** Moves every thread that is working here to a machine that stays on. */
  async leaving(to: string) {
    try {
      const left = await api.leaving(to);
      // The moved threads leave this list and show up on `to`; only what did not move needs saying.
      if (!left.moved.length && !left.failed.length) this.fail(new Error("Nothing is working here, so nothing moved."));
      for (const f of left.failed) this.fail(new Error(`${f.title} stayed: ${f.why}`));
    } catch (e) {
      this.fail(e);
    }
  }

  /** Lets agents run again; the turns the pause cut off carry on, then what was written meanwhile. */
  async resumeAll() {
    if (this.halting) return;
    this.halting = true;
    try {
      await api.resumeAll();
      this.halt = null;
    } catch (e) {
      this.fail(e);
    } finally {
      this.halting = false;
    }
  }

  /** Shows an error for a while; with `action`, a button that fixes it, and longer to reach it. */
  fail(e: unknown, action?: { label: string; run: () => unknown }) {
    const text = e instanceof Error ? e.message : String(e);
    this.error = text;
    this.errorAction = action ?? null;
    setTimeout(() => {
      if (this.error === text) {
        this.error = null;
        this.errorAction = null;
      }
    }, action ? 12000 : 6000);
  }
}

export const hub = new HubState();
// The layout check (scripts/check-layout.cjs) drives views through this in development.
if (import.meta.env.DEV) (window as any).hub = hub;
poll(() => (hub.now = Date.now()), 20_000);

/** Needs you and is still in front of you: not settled away, not snoozed. Every "needs you" count
 *  counts these, so a count never points at a thread the lists have put away. */
export function waitsOnYou(t: Thread): boolean {
  return hub.lists.waits(t);
}
