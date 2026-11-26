// The live event stream: connects, resumes after a drop and feeds events into the state.
import { quiet } from "./quiet";
import { notePulse } from "./pulse.svelte";
import { api, connectStream, type StreamItem } from "./api";
import { hubdFeatures } from "./compat";
import { notify } from "./native";
import { GUARD } from "./system";
import { poll } from "./poll";
import { notifyResources } from "./resource";
import { unread } from "./threadFlags";
import type { Event } from "./bindings/Event";
import type { WorkerRun } from "./bindings/WorkerRun";
import { isStandIn } from "./standIns.svelte";
import type { HubState } from "./state.svelte";

/** How many events the core returns per call of `GET /api/threads/{id}/events` (`default_limit`). */
const EVENT_PAGE = 5000;

/** The connection to the core: the stream's events, the catch-up when it falls behind and the
 *  watchdog that notices it went quiet. State stays on `hub`; this holds only what the stream needs. */
export class Stream {
  constructor(private hub: HubState) {}

  /** Events that arrived since the last frame, by thread. */
  arrived: Record<string, Event[]> = {};
  private frame = 0;
  /** The same flush on a timer: a window that is covered or minimised pauses animation frames, and
   *  the events would wait there until it is looked at again. */
  private frameTimer: ReturnType<typeof setTimeout> | undefined;
  /** When an event last came down the stream, for the watchdog in `start`. */
  private heard = 0;

  start() {
    const stop = connectStream({
      open: () => {
        this.hub.connected = true;
        this.resync();
      },
      close: () => (this.hub.connected = false),
      message: (m) => this.apply(m),
    });
    this.refreshHost();
    this.hub.refreshTailnet();
    const timer = poll(() => this.refreshHost(), 5000);
    const tailnet = poll(() => this.hub.refreshTailnet(), 60_000);
    // Other machines' problems do not come down this machine's stream, so ask now and then.
    const problems = poll(() => this.hub.refreshProblemsNeedYou(), 60_000);
    // A stream can go quiet without closing (a sleeping machine, a throttled window), and nothing
    // would say so: while the open thread runs and nothing has come for a while, read its log; and
    // when the window is looked at again, read everything.
    const watchdog = poll(() => {
      const id = this.hub.selected;
      if (!id || this.hub.view !== "thread" || !this.hub.connected || Date.now() - this.heard < 15_000) return;
      if (this.hub.threads.find((t) => t.id === id)?.status === "running") quiet(this.catchUp(id), "catch up");
    }, 15_000);
    let away = 0;
    const seen = () => {
      if (document.hidden) away = Date.now();
      else if (away && Date.now() - away > 5000) this.resync();
    };
    document.addEventListener("visibilitychange", seen);
    return () => {
      document.removeEventListener("visibilitychange", seen);
      watchdog();
      stop();
      timer();
      tailnet();
      problems();
    };
  }

  async resync() {
    notifyResources({ type: "resync" });
    try {
      quiet(api.version().then((v) => this.hub.onUpdate(v)), "version");
      quiet(api.halt().then((h) => (this.hub.halt = h)), "halt");
      quiet(api.projects().then((p) => (this.hub.projects = p)), "projects");
      quiet(api.desks().then((d) => (this.hub.desks = d.ids)), "desks");
      quiet(api.ideas().then((i) => (this.hub.ideas = i)), "ideas");
      quiet(api.usage(false).then((u) => this.hub.setUsage(u)), "usage");
      quiet(api.guard().then((g) => (this.hub.guard = g)), "guard");
      quiet(api.notes().then((n) => (this.hub.notes = n)), "notes");
      api.hosts().then((h) => {
        this.hub.hosts = h;
        this.hub.refreshBranches();
        this.hub.refreshProblemsNeedYou();
      }).catch(() => this.hub.refreshProblemsNeedYou());
      // Only a core that says so takes them; one that could not be asked keeps them with you.
      quiet(hubdFeatures().then((f) => (this.hub.guardReports = !!f?.has("guard.reports"))), "hubd features");
      this.hub.threads = this.hub.newest(this.hub.held(await api.threads()));
      this.hub.nameGuard();
      this.hub.loaded = true;
      this.hub.refreshBranches();
      await Promise.all(Object.keys(this.hub.events).filter((id) => !this.hub.holding.has(id)).map((id) => this.catchUp(id)));
    } catch (e) {
      this.hub.fail(e);
    }
  }

  async refreshHost() {
    try {
      this.hub.host = await api.host();
    } catch {
      // The connection state already tells the user the core is unreachable.
    }
  }

  apply(msg: StreamItem) {
    this.heard = Date.now();
    if (msg.type !== "resync") notifyResources(msg);
    if (msg.type === "resync") return void this.resync();
    if (msg.type === "draft") return this.hub.fromLink(msg.draft);
    if (msg.type === "update") return this.hub.onUpdate(msg.status);
    if (msg.type === "halt") return void (this.hub.halt = msg.halt);
    if (msg.type === "usage") return this.hub.setUsage({ plan: msg.plan });
    if (msg.type === "thread_deleted") return this.hub.forget(msg.id);
    if (msg.type === "aside") return void (this.hub.asides[msg.thread_id] = msg.aside);
    if (msg.type === "idea") {
      const i = this.hub.ideas.findIndex((x) => x.id === msg.idea.id);
      if (i >= 0) this.hub.ideas[i] = msg.idea;
      else this.hub.ideas.unshift(msg.idea);
      return;
    }
    if (msg.type === "problem") {
      this.hub.problemsTick++;
      const p = msg.problem;
      if (p.status === "open" && p.level === "error" && this.hub.view !== "problems" && !this.hub.problemsNew.includes(p.id)) this.hub.problemsNew = [...this.hub.problemsNew, p.id];
      // This machine's stream: its problems are keyed with no machine name.
      const key = `:${p.id}`;
      const needs = this.hub.problemsNeedYou.includes(key);
      if (p.needs_you && !needs) this.hub.problemsNeedYou = [...this.hub.problemsNeedYou, key];
      else if (!p.needs_you && needs) this.hub.problemsNeedYou = this.hub.problemsNeedYou.filter((k) => k !== key);
      return;
    }
    if (msg.type === "problem_news") {
      this.hub.problemNews = [{ id: msg.id, text: msg.text, ts: Date.now() }, ...this.hub.problemNews].slice(0, 20);
      if (msg.critical) void notify(GUARD, msg.text);
      return;
    }
    if (msg.type === "idea_removed") return void (this.hub.ideas = this.hub.ideas.filter((x) => x.id !== msg.id));
    if (msg.type === "project") {
      if (!this.hub.projects.some((p) => p.path === msg.project.path)) this.hub.projects.unshift(msg.project);
      return;
    }
    if (msg.type === "host") {
      const i = this.hub.hosts.findIndex((h) => h.name === msg.host.name);
      if (i < 0 || this.hub.hosts[i].state !== msg.host.state) this.hub.refreshBranches();
      if (i >= 0) this.hub.hosts[i] = msg.host;
      else this.hub.hosts.push(msg.host);
      return;
    }
    if (msg.type === "host_removed") {
      this.hub.hosts = this.hub.hosts.filter((h) => h.name !== msg.name);
      if (this.hub.hostFilter === msg.name) this.hub.hostFilter = null;
      return;
    }
    if (msg.type === "project_removed") {
      this.hub.projects = this.hub.projects.filter((p) => p.path !== msg.path);
      return;
    }
    if (msg.type === "thread") {
      // A thread on its way keeps the row it had until it has arrived.
      if (this.hub.holding.has(msg.thread.id)) return;
      // Events sent before this update land first, so a thread never names an engine its log
      // has not switched to yet.
      this.flush();
      const i = this.hub.threads.findIndex((t) => t.id === msg.thread.id);
      if (i >= 0) this.hub.announce(this.hub.threads[i], msg.thread);
      // A turn can move the branch, and a new thread has none known yet.
      if (i < 0 || this.hub.threads[i].status !== msg.thread.status) this.hub.refreshBranches();
      if (i >= 0) this.hub.threads[i] = msg.thread;
      else this.hub.threads.unshift(msg.thread);
      this.hub.threads.sort((a, b) => b.updated_at - a.updated_at || (a.id < b.id ? -1 : 1));
      this.hub.nameGuard();
      // Coming back while you look at it is seen.
      if (this.hub.view === "thread" && this.hub.selected === msg.thread.id && unread(msg.thread) && !document.hidden) this.hub.markSeen(msg.thread.id);
      return;
    }
    // A notice from outside (`POST /api/notices`) has no page of its own yet; only events go on.
    if (msg.type === "notice") return;
    // The update status carries the rollback; this only tells a shell to follow `current`.
    if (msg.type === "rolled_back") return;
    const id = msg.event.thread_id;
    if (this.hub.holding.has(id)) return;
    notePulse(msg.event);
    if (msg.event.kind === "worker_run") {
      const { lines: _lines, ...run } = msg.event.data as WorkerRun & { lines?: unknown };
      const have = this.hub.workerRuns[id];
      // A running run wins over one that ended, as in the core.
      if (!have || have.run === run.run || run.state === "running" || have.state !== "running") this.hub.workerRuns = { ...this.hub.workerRuns, [id]: run };
    }
    const log = this.hub.events[id];
    if (!log) return;
    const queue = this.arrived[id];
    const last = (queue ?? log).at(-1)?.seq ?? 0;
    if (msg.event.seq === last + 1) {
      if (queue) queue.push(msg.event);
      else this.arrived[id] = [msg.event];
      this.frame ||= requestAnimationFrame(() => this.flush());
      this.frameTimer ??= setTimeout(() => this.flush(), 250);
    } else if (msg.event.seq > last + 1) this.catchUp(id);
  }

  /** Adds the events that arrived since the last frame, so a burst of output folds once. */
  flush() {
    cancelAnimationFrame(this.frame);
    this.frame = 0;
    clearTimeout(this.frameTimer);
    this.frameTimer = undefined;
    const arrived = Object.entries(this.arrived);
    if (!arrived.length) return;
    this.arrived = {};
    const next = { ...this.hub.events };
    for (const [id, list] of arrived) if (next[id]) next[id] = next[id].concat(list);
    this.hub.events = next;
  }

  async catchUp(id: string) {
    if (isStandIn(id)) return;
    const log = this.hub.events[id] ?? [];
    // The core hands over at most one page (5000 events) per call, so a long log is fetched page by
    // page; stopping after the first left a thread's end unseen (a finished side job still "working").
    const fresh: Event[] = [];
    for (let after = log.at(-1)?.seq ?? 0; ; ) {
      const page = await api.events(id, after);
      fresh.push(...page);
      if (page.length < EVENT_PAGE) break;
      after = page[page.length - 1].seq;
    }
    this.flush();
    const merged = this.hub.events[id] ?? [];
    const last = merged.at(-1)?.seq ?? 0;
    const news = fresh.filter((e) => e.seq > last);
    // Nothing new: leave the log as it is, so nothing that reads it is drawn again.
    if (news.length || !this.hub.events[id]) this.hub.events = { ...this.hub.events, [id]: merged.concat(news) };
  }
}
