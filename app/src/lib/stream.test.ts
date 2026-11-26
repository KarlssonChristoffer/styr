// Tests for the pure part of Stream.apply (stream.svelte.ts): how a message from the core's stream
// changes the state held on `hub`, with a plain object standing in for it.
import { beforeEach, describe, expect, it, vi } from "vitest";

const events = vi.fn();
vi.mock("./api", () => ({ api: { events: (...a: unknown[]) => events(...a) }, connectStream: vi.fn() }));
vi.mock("./pulse.svelte", () => ({ notePulse: vi.fn() }));
vi.mock("./native", () => ({ notify: vi.fn() }));
vi.mock("./standIns.svelte", () => ({ isStandIn: (id: string) => id.startsWith("standin:") }));
vi.mock("./compat", () => ({ hubdFeatures: vi.fn() }));

import { Stream } from "./stream.svelte";

const ev = (thread_id: string, seq: number, kind = "update") => ({ thread_id, seq, ts: seq, kind, data: {} });
const thread = (id: string, over: Record<string, unknown> = {}) => ({ id, status: "idle", updated_at: 1, returned_at: null, seen_at: null, ...over });

function setup(over: Record<string, unknown> = {}) {
  const hub: any = {
    threads: [],
    events: {},
    ideas: [],
    projects: [],
    hosts: [],
    asides: {},
    holding: new Set<string>(),
    workerRuns: {},
    problemNews: [],
    problemsTick: 0,
    problemsNew: [],
    problemsNeedYou: [],
    view: "home",
    selected: null,
    hostFilter: null,
    halt: null,
    forget: vi.fn(),
    announce: vi.fn(),
    refreshBranches: vi.fn(),
    nameGuard: vi.fn(),
    markSeen: vi.fn(),
    onUpdate: vi.fn(),
    setUsage: vi.fn(),
    fromLink: vi.fn(),
    ...over,
  };
  return { hub, stream: new Stream(hub) };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("requestAnimationFrame", (cb: () => void) => setTimeout(cb, 16) as unknown as number);
  vi.stubGlobal("cancelAnimationFrame", (n: number) => clearTimeout(n));
  vi.stubGlobal("document", { hidden: false });
  events.mockReset();
});

describe("Stream.apply: simple state", () => {
  it("keeps halt, asides and thread removal", () => {
    const { hub, stream } = setup();
    stream.apply({ type: "halt", halt: { x: 1 } } as any);
    expect(hub.halt).toEqual({ x: 1 });
    stream.apply({ type: "aside", thread_id: "t", aside: { a: 1 } } as any);
    expect(hub.asides.t).toEqual({ a: 1 });
    stream.apply({ type: "thread_deleted", id: "t" } as any);
    expect(hub.forget).toHaveBeenCalledWith("t");
  });

  it("replaces an idea it has and puts a new one first; removes by id", () => {
    const { hub, stream } = setup({ ideas: [{ id: "a", text: "old" }] });
    stream.apply({ type: "idea", idea: { id: "a", text: "new" } } as any);
    stream.apply({ type: "idea", idea: { id: "b", text: "b" } } as any);
    expect(hub.ideas.map((i: any) => i.text)).toEqual(["b", "new"]);
    stream.apply({ type: "idea_removed", id: "a" } as any);
    expect(hub.ideas.map((i: any) => i.id)).toEqual(["b"]);
  });

  it("adds a project once and removes it by path", () => {
    const { hub, stream } = setup();
    const project = { path: "/p" };
    stream.apply({ type: "project", project } as any);
    stream.apply({ type: "project", project } as any);
    expect(hub.projects).toHaveLength(1);
    stream.apply({ type: "project_removed", path: "/p" } as any);
    expect(hub.projects).toEqual([]);
  });

  it("adds and updates hosts, refreshing branches only when the state changes", () => {
    const { hub, stream } = setup();
    stream.apply({ type: "host", host: { name: "pc", state: "up" } } as any);
    expect(hub.refreshBranches).toHaveBeenCalledTimes(1);
    stream.apply({ type: "host", host: { name: "pc", state: "up", extra: 1 } } as any);
    expect(hub.refreshBranches).toHaveBeenCalledTimes(1);
    stream.apply({ type: "host", host: { name: "pc", state: "down" } } as any);
    expect(hub.refreshBranches).toHaveBeenCalledTimes(2);
    expect(hub.hosts).toEqual([{ name: "pc", state: "down" }]);
    hub.hostFilter = "pc";
    stream.apply({ type: "host_removed", name: "pc" } as any);
    expect(hub.hosts).toEqual([]);
    expect(hub.hostFilter).toBeNull();
  });

  it("marks an error problem new once, and tracks whether it needs you", () => {
    const { hub, stream } = setup();
    const p = (needs_you: boolean) => ({ type: "problem", problem: { id: "p1", status: "open", level: "error", needs_you } }) as any;
    stream.apply(p(true));
    stream.apply(p(true));
    expect(hub.problemsNew).toEqual(["p1"]);
    expect(hub.problemsNeedYou).toEqual([":p1"]);
    expect(hub.problemsTick).toBe(2);
    stream.apply(p(false));
    expect(hub.problemsNeedYou).toEqual([]);
  });

  it("does not mark a problem new while the problems page is open", () => {
    const { hub, stream } = setup({ view: "problems" });
    stream.apply({ type: "problem", problem: { id: "p1", status: "open", level: "error", needs_you: false } } as any);
    expect(hub.problemsNew).toEqual([]);
  });

  it("keeps the 20 newest problem news, newest first", () => {
    const { hub, stream } = setup();
    for (let i = 0; i < 25; i++) stream.apply({ type: "problem_news", id: String(i), text: "t", critical: false } as any);
    expect(hub.problemNews).toHaveLength(20);
    expect(hub.problemNews[0].id).toBe("24");
  });
});

describe("Stream.apply: threads", () => {
  it("adds a new thread, sorts by newest update, and refreshes branches", () => {
    const { hub, stream } = setup({ threads: [thread("a", { updated_at: 5 })] });
    stream.apply({ type: "thread", thread: thread("b", { updated_at: 9 }) } as any);
    expect(hub.threads.map((t: any) => t.id)).toEqual(["b", "a"]);
    expect(hub.refreshBranches).toHaveBeenCalled();
    expect(hub.nameGuard).toHaveBeenCalled();
  });

  it("breaks ties by id", () => {
    const { hub, stream } = setup({ threads: [thread("b", { updated_at: 5 })] });
    stream.apply({ type: "thread", thread: thread("a", { updated_at: 5 }) } as any);
    expect(hub.threads.map((t: any) => t.id)).toEqual(["a", "b"]);
  });

  it("announces a change to a known thread and only refreshes branches when its status moved", () => {
    const { hub, stream } = setup({ threads: [thread("a")] });
    stream.apply({ type: "thread", thread: thread("a", { updated_at: 2 }) } as any);
    expect(hub.announce).toHaveBeenCalledTimes(1);
    expect(hub.refreshBranches).not.toHaveBeenCalled();
    stream.apply({ type: "thread", thread: thread("a", { status: "running", updated_at: 3 }) } as any);
    expect(hub.refreshBranches).toHaveBeenCalledTimes(1);
  });

  it("leaves a thread that is on its way alone", () => {
    const { hub, stream } = setup({ holding: new Set(["a"]) });
    stream.apply({ type: "thread", thread: thread("a") } as any);
    expect(hub.threads).toEqual([]);
  });

  it("marks an unread thread seen when you are looking at it", () => {
    const { hub, stream } = setup({ view: "thread", selected: "a" });
    stream.apply({ type: "thread", thread: thread("a", { returned_at: 5 }) } as any);
    expect(hub.markSeen).toHaveBeenCalledWith("a");
  });

  it("does not mark it seen when the window is hidden", () => {
    vi.stubGlobal("document", { hidden: true });
    const { hub, stream } = setup({ view: "thread", selected: "a" });
    stream.apply({ type: "thread", thread: thread("a", { returned_at: 5 }) } as any);
    expect(hub.markSeen).not.toHaveBeenCalled();
  });
});

describe("Stream.apply: events", () => {
  it("ignores events for a thread whose log is not loaded", () => {
    const { hub, stream } = setup();
    stream.apply({ type: "event", event: ev("a", 1) } as any);
    vi.advanceTimersByTime(1000);
    expect(hub.events).toEqual({});
    expect(events).not.toHaveBeenCalled();
  });

  it("queues the next event in order and adds a burst in one go", () => {
    const { hub, stream } = setup({ events: { a: [ev("a", 1)] } });
    const before = hub.events;
    stream.apply({ type: "event", event: ev("a", 2) } as any);
    stream.apply({ type: "event", event: ev("a", 3) } as any);
    expect(hub.events).toBe(before);
    vi.advanceTimersByTime(300);
    expect(hub.events.a.map((e: any) => e.seq)).toEqual([1, 2, 3]);
  });

  it("flush adds what arrived straight away", () => {
    const { hub, stream } = setup({ events: { a: [] } });
    stream.apply({ type: "event", event: ev("a", 1) } as any);
    stream.flush();
    expect(hub.events.a).toHaveLength(1);
    expect(stream.arrived).toEqual({});
  });

  it("catches up from the log when an event skips ahead", async () => {
    const { hub, stream } = setup({ events: { a: [ev("a", 1)] } });
    events.mockResolvedValue([ev("a", 2), ev("a", 3), ev("a", 4)]);
    stream.apply({ type: "event", event: ev("a", 4) } as any);
    await vi.waitFor(() => expect(hub.events.a).toHaveLength(4));
    expect(events).toHaveBeenCalledWith("a", 1);
  });

  it("drops a stale event it already has", () => {
    const { hub, stream } = setup({ events: { a: [ev("a", 1), ev("a", 2)] } });
    stream.apply({ type: "event", event: ev("a", 2) } as any);
    vi.advanceTimersByTime(300);
    expect(hub.events.a).toHaveLength(2);
    expect(events).not.toHaveBeenCalled();
  });

  it("keeps the latest worker run, but a finished run does not replace a running one", () => {
    const { hub, stream } = setup();
    const run = (r: number, state: string) => ({ type: "event", event: { ...ev("a", 1, "worker_run"), data: { run: r, state, lines: ["x"] } } }) as any;
    stream.apply(run(1, "running"));
    expect(hub.workerRuns.a).toEqual({ run: 1, state: "running" });
    stream.apply(run(2, "done"));
    expect(hub.workerRuns.a.run).toBe(1);
    stream.apply(run(2, "running"));
    expect(hub.workerRuns.a.run).toBe(2);
  });

  it("ignores events for a held thread, and notices", () => {
    const { hub, stream } = setup({ holding: new Set(["a"]), events: { a: [] } });
    stream.apply({ type: "event", event: ev("a", 1) } as any);
    stream.apply({ type: "notice" } as any);
    vi.advanceTimersByTime(300);
    expect(hub.events.a).toEqual([]);
  });
});

describe("Stream.catchUp", () => {
  it("fetches page by page and merges only what is newer", async () => {
    const { hub, stream } = setup({ events: { a: [ev("a", 1)] } });
    const page = Array.from({ length: 5000 }, (_, i) => ev("a", i + 2));
    events.mockResolvedValueOnce(page).mockResolvedValueOnce([ev("a", 5002)]);
    await stream.catchUp("a");
    expect(events).toHaveBeenNthCalledWith(1, "a", 1);
    expect(events).toHaveBeenNthCalledWith(2, "a", 5001);
    expect(hub.events.a).toHaveLength(5002);
  });

  it("leaves the log untouched when nothing is new", async () => {
    const { hub, stream } = setup({ events: { a: [ev("a", 1)] } });
    const before = hub.events;
    events.mockResolvedValue([]);
    await stream.catchUp("a");
    expect(hub.events).toBe(before);
  });

  it("does nothing for a stand-in thread", async () => {
    const { stream } = setup();
    await stream.catchUp("standin:1");
    expect(events).not.toHaveBeenCalled();
  });
});
