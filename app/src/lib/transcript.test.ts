// First tests for transcript.ts: how a thread's event log folds into the items the transcript shows.
import { describe, expect, it } from "vitest";
import type { Event } from "./bindings/Event";
import { IDLE_BREAK_MS, attemptOf, diffLines, fold, folded, jobWork, proposalIn, questionIn, queued, ruleIn } from "./transcript";
import { designOn } from "./transcriptItems";

let seq = 0;
const ev = (kind: string, data: Record<string, unknown> = {}, ts = 1000 + seq * 10): Event =>
  ({ thread_id: "t", seq: ++seq, ts, kind, data }) as unknown as Event;
const update = (data: Record<string, unknown>, ts?: number) => ev("update", data, ts);
const said = (text: string, ts?: number) => ev("user_message", { text }, ts);
const chunk = (text: string, u = "agent_message_chunk") => update({ sessionUpdate: u, content: { type: "text", text } });
const kinds = (items: { kind: string }[]) => items.map((i) => i.kind);

describe("fold", () => {
  it("gives an empty thread no items", () => {
    expect(fold([])).toEqual([]);
  });

  it("joins message chunks into one reply", () => {
    seq = 0;
    const items = fold([said("hej"), chunk("Hel"), chunk("lo "), chunk("world")]);
    expect(kinds(items)).toEqual(["user", "assistant"]);
    expect(items[1]).toMatchObject({ text: "Hello world" });
  });

  it("keeps thoughts and replies apart", () => {
    seq = 0;
    const items = fold([said("x"), chunk("hmm", "agent_thought_chunk"), chunk("ok"), chunk("more", "agent_thought_chunk")]);
    expect(kinds(items)).toEqual(["user", "thought", "assistant", "thought"]);
  });

  it("ignores chunks that carry no text", () => {
    seq = 0;
    const items = fold([said("x"), update({ sessionUpdate: "agent_message_chunk", content: { type: "resource" } })]);
    expect(items[1]).toMatchObject({ kind: "assistant", text: "" });
  });

  it("makes an image of an image chunk", () => {
    seq = 0;
    const items = fold([update({ sessionUpdate: "agent_message_chunk", content: { type: "image", data: "AAAA", mimeType: "image/jpeg" } })]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "image", src: "data:image/jpeg;base64,AAAA" });
  });

  it("collects terminal text typed outside Hub into one user item", () => {
    seq = 0;
    const items = fold([update({ sessionUpdate: "user_message_chunk", content: { type: "text", text: "ls " } }), update({ sessionUpdate: "user_message_chunk", content: { type: "text", text: "-la" } })]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "user", terminal: true, text: "ls -la" });
  });

  it("reads user message fields", () => {
    seq = 0;
    const [item] = fold([ev("user_message", { text: "Always run the tests first.", attachments: [{ name: "a" }] })]);
    expect(item).toMatchObject({ kind: "user", text: "Always run the tests first.", rule: "Always run the tests first." });
    expect((item as any).attachments).toHaveLength(1);
  });

  it("marks a message from Hub itself", () => {
    seq = 0;
    const [item] = fold([ev("user_message", { text: "continue", restart: {} })]);
    expect(item).toMatchObject({ kind: "user", fromHub: "restart" });
  });
});

describe("tool calls", () => {
  const call = (data: Record<string, unknown>) => update({ sessionUpdate: "tool_call", toolCallId: "c1", ...data });
  const upd = (data: Record<string, unknown>, ts?: number) => update({ sessionUpdate: "tool_call_update", toolCallId: "c1", ...data }, ts);

  it("merges updates into the one tool item", () => {
    seq = 0;
    const items = fold([call({ title: "Run ls", status: "pending", rawInput: { command: "ls" } }), upd({ status: "completed" }, 5000)]);
    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({ kind: "tool", title: "Run ls", status: "completed", command: "ls" });
    expect((items[0] as any).until).toBe(5000);
  });

  it("falls back to a default title and status", () => {
    seq = 0;
    const [item] = fold([call({})]);
    expect(item).toMatchObject({ title: "Tool", status: "pending" });
  });

  it("appends streamed output and records the exit code", () => {
    seq = 0;
    const items = fold([
      call({ title: "Run", status: "in_progress", rawInput: { command: "make" } }),
      upd({ _meta: { terminal_output_delta: { data: "one\n" } } }),
      upd({ _meta: { terminal_output_delta: { data: "two\n" } } }),
      upd({ status: "completed", _meta: { terminal_exit: { exit_code: 2 } } }),
    ]);
    expect(items[0]).toMatchObject({ output: "one\ntwo\n", exit: 2, terminal: true });
  });

  it("keeps only the end of very long output and says it was cut", () => {
    seq = 0;
    const big = "x".repeat(300 * 1024);
    const items = fold([call({ title: "Run" }), upd({ _meta: { terminal_output_delta: { data: big } } }), upd({ _meta: { terminal_output_delta: { data: big } } })]);
    const tool = items[0] as any;
    expect(tool.cut).toBe(true);
    expect(tool.output.length).toBeLessThanOrEqual(2 * 256 * 1024);
  });

  it("joins the argument list of a Codex command", () => {
    seq = 0;
    const [item] = fold([call({ title: "Run", rawInput: { command: ["git", "status"] } })]);
    expect((item as any).command).toBe("git status");
  });

  it("shows the same notice only once", () => {
    seq = 0;
    const note = () => update({ sessionUpdate: "notice", title: "Heads up" });
    expect(kinds(fold([note(), note()]))).toEqual(["notice"]);
  });
});

describe("plans", () => {
  it("updates a plan in place and stamps when entries started and finished", () => {
    seq = 0;
    const plan = (status: string, ts: number) => update({ sessionUpdate: "plan", entries: [{ content: "a", status }] }, ts);
    const items = fold([said("go"), plan("pending", 100), plan("in_progress", 200), plan("completed", 300)]);
    expect(kinds(items)).toEqual(["user", "plan"]);
    expect(items[1]).toMatchObject({ entries: [{ content: "a", status: "completed" }], marks: [{ from: 200, to: 300 }] });
  });

  it("starts a new plan after a message of yours", () => {
    seq = 0;
    const plan = update({ sessionUpdate: "plan", entries: [{ content: "a", status: "pending" }] });
    expect(kinds(fold([plan, said("again"), plan]))).toEqual(["plan", "user", "plan"]);
  });
});

describe("permissions", () => {
  it("resolves a request with the chosen option", () => {
    seq = 0;
    const items = fold([
      ev("permission_request", { id: "p1", tool_call: { toolCallId: "c1", title: "Run rm", rawInput: { command: "ls" } }, options: [{ id: "allow" }] }),
      ev("permission_resolved", { id: "p1", option_id: "allow" }),
    ]);
    expect(items[0]).toMatchObject({ kind: "permission", id: "p1", resolved: true, answer: "allow", command: "ls" });
  });

  it("says why an automatic refusal happened, and stays quiet about allowed ones", () => {
    seq = 0;
    const items = fold([ev("permission_auto", { allowed: true }), ev("permission_auto", { allowed: false, tool_call: { title: "Delete" }, why: "locked" })]);
    expect(items).toEqual([{ kind: "refused", title: "Delete", why: "locked" }]);
  });

  it("ignores a resolution for an unknown request", () => {
    seq = 0;
    expect(fold([ev("permission_resolved", { id: "nope", option_id: "x" })])).toEqual([]);
  });
});

describe("turns and file changes", () => {
  const edit = (id: string, path: string, oldText: string, newText: string, status = "completed") =>
    update({ sessionUpdate: "tool_call", toolCallId: id, title: "Edit", status, content: [{ type: "diff", path, oldText, newText }] });

  it("ends a turn with the files it changed, grouped by path", () => {
    seq = 0;
    const items = fold([said("fix"), edit("a", "x.ts", "1", "2"), edit("b", "x.ts", "3", "4"), edit("c", "y.ts", "", "new"), ev("turn_end", { stop_reason: "end_turn" })]);
    expect(kinds(items)).toEqual(["user", "tool", "tool", "tool", "changes", "turn"]);
    const changes = items.find((i) => i.kind === "changes") as any;
    expect(changes.files.map((f: any) => [f.path, f.edits.length])).toEqual([["x.ts", 2], ["y.ts", 1]]);
  });

  it("leaves failed edits out", () => {
    seq = 0;
    const items = fold([said("fix"), edit("a", "x.ts", "1", "2", "failed"), ev("turn_end", {})]);
    expect(kinds(items)).not.toContain("changes");
  });

  it("starts a fresh set of changes at each message", () => {
    seq = 0;
    const items = fold([said("one"), edit("a", "x.ts", "1", "2"), said("two"), ev("turn_end", {})]);
    expect(kinds(items)).not.toContain("changes");
  });

  it("notes a turn that ended for another reason than end_turn", () => {
    seq = 0;
    const items = fold([said("go"), ev("turn_end", { stop_reason: "max_tokens" })]);
    expect(items.at(-1)).toEqual({ kind: "end", reason: "max_tokens" });
  });

  it("shows the running turn's files through jobWork before it ends", () => {
    seq = 0;
    const events = [said("fix"), edit("a", "x.ts", "1", "2")];
    const work = jobWork(events);
    expect(work.files).toEqual([{ path: "x.ts", edits: [{ old: "1", new: "2" }] }]);
    expect(work.step).toMatchObject({ kind: "tool", id: "a" });
  });
});

describe("message queue", () => {
  it("holds a message sent while a turn runs until its turn starts", () => {
    seq = 0;
    const first = said("one");
    const start1 = ev("turn_start", { prompt: first.seq });
    const second = said("two");
    const events = [first, start1, second];
    expect(kinds(fold(events))).toEqual(["user"]);
    expect(queued(events).map((q) => q.text)).toEqual(["two"]);
    const more = [...events, ev("turn_end", {}), ev("turn_start", { prompt: second.seq })];
    expect(fold(more).map((i) => (i as any).text ?? i.kind)).toEqual(["one", "turn", "two"]);
    expect(queued(more)).toEqual([]);
  });

  it("removes a cancelled queued message and edits one in place", () => {
    seq = 0;
    const first = said("one");
    const events = [first, ev("turn_start", { prompt: first.seq }), said("two"), said("three")];
    const withEdit = [...events, ev("queue", { seq: events[2].seq, action: "edited", text: "two!" }), ev("queue", { seq: events[3].seq, action: "cancelled" })];
    expect(queued(withEdit).map((q) => [q.text, q.edited])).toEqual([["two!", true]]);
  });

  it("shows a dropped message as sent", () => {
    seq = 0;
    const first = said("one");
    const second = said("two");
    const events = [first, ev("turn_start", { prompt: first.seq }), second, ev("queue", { seq: second.seq, action: "dropped" })];
    expect(fold(events).map((i) => (i as any).text)).toEqual(["one", "two"]);
    expect(queued(events)).toEqual([]);
  });

  it("stamps the read receipt as the turn begins and the agent answers", () => {
    seq = 0;
    const first = ev("user_message", { text: "hi" }, 100);
    const items = fold([first, ev("turn_start", { prompt: first.seq }, 200), update({ sessionUpdate: "agent_message_chunk", content: { type: "text", text: "yo" } }, 300)]);
    expect((items[0] as any).receipt).toEqual({ got: 100, taken: 200, seen: 300 });
  });
});

describe("breaks in time", () => {
  it("adds an idle break after a long pause since the last turn ended", () => {
    seq = 0;
    const items = fold([said("a", 1000), ev("turn_end", {}, 1500), said("b", 1500 + IDLE_BREAK_MS)]);
    expect(kinds(items)).toEqual(["user", "turn", "idle", "user"]);
    expect(items[2]).toMatchObject({ gap: IDLE_BREAK_MS });
  });

  it("adds no idle break for a short pause", () => {
    seq = 0;
    expect(kinds(fold([said("a", 1000), said("b", 2000)]))).not.toContain("idle");
  });

  it("puts one New break before the first thing the agent did after you last looked", () => {
    seq = 0;
    const events = [said("a", 1000), update({ sessionUpdate: "agent_message_chunk", content: { type: "text", text: "x" } }, 2000), update({ sessionUpdate: "tool_call", toolCallId: "c", title: "T" }, 3000)];
    const items = folded(events, 1500);
    expect(kinds(items)).toEqual(["user", "new", "assistant", "tool"]);
    expect(kinds(folded(events, 5000))).not.toContain("new");
    expect(kinds(folded(events))).not.toContain("new");
  });
});

describe("folded", () => {
  it("returns the same items for the same events array", () => {
    seq = 0;
    const events = [said("a"), chunk("b")];
    expect(folded(events)).toBe(folded(events));
  });

  it("copes with unknown event kinds", () => {
    seq = 0;
    expect(fold([ev("something_new_from_the_future", { a: 1 })])).toEqual([]);
  });
});

describe("diffLines", () => {
  it("marks added and removed lines with context and an ellipsis for the rest", () => {
    const before = ["1", "2", "3", "4", "5", "6", "7", "8"].join("\n");
    const after = ["1", "2", "3", "4", "five", "6", "7", "8"].join("\n");
    expect(diffLines(before, after, 1)).toEqual([
      { op: " ", text: "…" },
      { op: " ", text: "4" },
      { op: "-", text: "5" },
      { op: "+", text: "five" },
      { op: " ", text: "6" },
      { op: " ", text: "…" },
    ]);
  });

  it("treats an empty side as all added or all removed", () => {
    expect(diffLines("", "a\nb")).toEqual([{ op: "+", text: "a" }, { op: "+", text: "b" }]);
    expect(diffLines("a", "")).toEqual([{ op: "-", text: "a" }]);
  });

  it("shows no change as a single ellipsis", () => {
    expect(diffLines("a\nb", "a\nb")).toEqual([{ op: " ", text: "…" }]);
  });

  it("falls back to remove-all add-all for very large inputs", () => {
    const a = Array.from({ length: 700 }, (_, i) => `a${i}`).join("\n");
    const b = Array.from({ length: 700 }, (_, i) => `b${i}`).join("\n");
    const out = diffLines(a, b);
    expect(out.filter((l) => l.op === "-")).toHaveLength(700);
    expect(out.filter((l) => l.op === "+")).toHaveLength(700);
  });
});

describe("text helpers", () => {
  it("ruleIn skips slash commands and short or missing rules", () => {
    expect(ruleIn("/clear always")).toBeUndefined();
    expect(ruleIn("Please add a button.")).toBeUndefined();
    expect(ruleIn("Never push to main without asking me first.")).toBe("Never push to main without asking me first.");
  });

  it("proposalIn finds a closing yes/no proposal", () => {
    expect(proposalIn("Done.\n\nShall I open a pull request?")).toBe("Shall I open a pull request?");
    expect(proposalIn("Should I use A or B?")).toBeNull();
    expect(proposalIn("Here is the result.")).toBeNull();
    expect(proposalIn("Two lines\nShould I go on?")).toBeNull();
  });

  it("questionIn reads a lettered list under a question", () => {
    const q = questionIn("Which one do you want?\n\nA. The fast one\nB. The safe one");
    expect(q).toMatchObject({ options: ["The fast one", "The safe one"] });
    expect(questionIn("Here are things:\n1. one\n2. two")).toBeNull();
    expect(questionIn("Which?\n1. only")).toBeNull();
  });
});

describe("an event whose payload is not an object", () => {
  const bare = (kind: string) => ({ ...ev(kind), data: null }) as unknown as Event;

  it("folds as one with no fields", () => {
    const items = fold([said("go"), bare("restart"), bare("paused"), bare("checkpoint")]);
    expect(kinds(items)).toEqual(["user", "restart", "paused"]);
    expect(items[1]).toMatchObject({ kind: "restart", by: "crash", queued: 0 });
  });

  it("is passed over by the readers of one kind", () => {
    expect(attemptOf([bare("attempt")])).toBeNull();
    expect(designOn([ev("design", { on: true }), bare("design")])).toBe(true);
  });
});
