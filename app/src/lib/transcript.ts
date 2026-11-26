// Folds the raw event log of a thread into the items the thread view draws.
import type { PluginRepo } from "./bindings/PluginRepo";
import type { Attachment } from "./bindings/Attachment";
import type { Event } from "./bindings/Event";
import type { WorkerLine } from "./bindings/WorkerLine";
import type { WorkerRun } from "./bindings/WorkerRun";
import { toolRisk, type Risk } from "./risk";

import { IDLE_BREAK_MS, askLead, type AttemptStart, type Carried, type ChildItem, type DesignReviewItem, type DocsState, type ErrorItem, type FileChange, type ImageItem, type Item, type Offer, type PageItem, type PermissionItem, type ProofState, type QueuedItem, type ReviewItem, type TaskItem, type ToolItem } from "./transcriptItems";
export * from "./transcriptItems";
export * from "./transcriptText";
export * from "./transcriptTools";
import { outputOnly, addOutput, commandOf, defined, image, toolDetail } from "./transcriptTools";
import { ruleIn } from "./transcriptText";
import { known, payload, typed } from "./events";

/** A model's name without the engine's note on it: "Default (recommended)" is "Default". */
const plain = (name?: string | null) => name?.replace(/\s*\(recommended\)\s*$/i, "") || undefined;

/** Folds an event log once, however many views ask: logs are replaced, not changed, when events arrive. */
const folds = new WeakMap<Event[], Map<number, { items: Item[]; queue: QueuedItem[]; pending: FileChange[] }>>();
function foldOnce(events: Event[], since?: number) {
  let by = folds.get(events);
  if (!by) folds.set(events, (by = new Map()));
  let f = by.get(since ?? -1);
  if (!f) by.set(since ?? -1, (f = foldAll(events, since)));
  return f;
}
/** `since`: when you last looked at the thread; what the agent did after it gets a "New" break. */
export function folded(events: Event[], since?: number): Item[] {
  return foldOnce(events, since).items;
}

/** What a thread has changed so far, file by file, including the turn still running (whose files
 *  join the transcript only when it ends), and the step it is on now. A side job's line reads it. */
export function jobWork(events: Event[]): { files: FileChange[]; step: Extract<Item, { kind: "tool" }> | null } {
  const { items, pending } = foldOnce(events);
  const byPath = new Map<string, FileChange>();
  const add = (f: FileChange) => {
    const have = byPath.get(f.path);
    if (have) have.edits = [...have.edits, ...f.edits];
    else byPath.set(f.path, { path: f.path, edits: [...f.edits] });
  };
  for (const i of items) if (i.kind === "changes") i.files.forEach(add);
  pending.forEach(add);
  const step = items.findLast((i): i is Extract<Item, { kind: "tool" }> => i.kind === "tool") ?? null;
  return { files: [...byPath.values()], step };
}

/** Messages waiting behind the running turn, oldest first. They join the transcript when the
 *  engine takes them, so the conversation reads in the order it happened. */
export function queued(events: Event[]): QueuedItem[] {
  return foldOnce(events).queue;
}


type Json = Record<string, any>;

export function fold(events: Event[]): Item[] {
  return foldAll(events).items;
}

function foldAll(events: Event[], since?: number): { items: Item[]; queue: QueuedItem[]; pending: FileChange[] } {
  const items: Item[] = [];
  // The queue: a message sent while a turn runs (or while earlier ones wait) waits here until its
  // own turn starts. Only logs that mark turn starts can tell; older parts show as they were.
  const held = new Map<number, QueuedItem>();
  const marked = events.find((e) => e.kind === "turn_start")?.seq ?? Infinity;
  // The message whose turn is running, and one sent while idle whose turn has not started yet.
  let running: number | null = null;
  let next: number | null = null;
  // The message whose turn is running, for its read receipt.
  let reading: QueuedItem | undefined;
  // When you or the agent last said something, for the "New" break after a long pause.
  let lastActive = 0;
  const tools = new Map<string, ToolItem>();
  const permissions = new Map<string, PermissionItem>();
  const reviews = new Map<string, ReviewItem>();
  const designs = new Map<string, DesignReviewItem>();
  // The design review whose findings the agent is working on, until that turn ends.
  let answeringDesign: string | undefined;
  const children = new Map<string, ChildItem>();
  const tasks = new Map<string, TaskItem>();
  const pages = new Map<string, PageItem>();
  // The review whose findings the agent is working on, until that turn ends.
  let answering: string | undefined;
  // Everything known about each tool call, merged, since a permission request may only carry part.
  const calls = new Map<string, Json>();
  // Tool calls whose picture is already in the list; updates repeat their content.
  const shownImages = new Set<string>();
  // Edits made since the last user message, by tool call, shown together when the turn ends.
  let edits = new Map<string, { path: string; old: string; new: string }[]>();
  // The replies the agent offered in the running turn (`offer_replies`), for its end.
  let offered: Offer | undefined;
  const last = () => items.at(-1);
  // The "New" break goes in once, before the first thing the agent did after `since`.
  let newMarked = since == null;
  let newPlaced = false;
  const agentDid = (e: Event) => {
    if (e.kind !== "update") return false;
    const u = (e.data as Json)?.sessionUpdate as string | undefined;
    return u === "agent_message_chunk" || u === "tool_call" || u === "plan";
  };
  // Clashes noticed mid-reply wait for the reply to end, so they do not cut its text in two.
  let clashes: Extract<Item, { kind: "clash" }>[] = [];
  const flush = () => {
    items.push(...clashes);
    clashes = [];
  };

  // A message joins the transcript: what came before it is over, its own turn begins.
  const start = (said: QueuedItem) => {
    flush();
    edits = new Map();
    answering = said.review;
    answeringDesign = said.designReview;
    items.push(said);
  };

  for (const e of events) {
    const { kind, d } = typed(e);
    if (!newMarked && e.ts > since! && agentDid(e)) {
      newMarked = newPlaced = true;
      flush();
      items.push({ kind: "new", since: since!, ts: e.ts });
    }
    switch (kind) {
      case "user_message": {
        const said = userItem(e);
        // You wrote something after you came back: from there on you were looking.
        if (!newMarked && e.ts > since! && !said.fromHub && !said.sentBy) newMarked = true;
        // Carrying on after a restart goes first, ahead of what waited.
        const busy = e.seq > marked && (e.data as Json).restart == null && (running != null || next != null || held.size > 0);
        if (busy) held.set(e.seq, said);
        else {
          // One "New" is enough: the one for what you have not seen stands in for the pause.
          if (!said.fromHub && lastActive && e.ts - lastActive >= IDLE_BREAK_MS && !newPlaced) {
            flush();
            items.push({ kind: "idle", gap: e.ts - lastActive, ts: e.ts });
          }
          if (!said.fromHub) lastActive = e.ts;
          start(said);
          next = e.seq;
          // In and next: the tick shows now, not once the engine is up to take it.
          said.receipt = { got: e.ts };
          reading = said;
        }
        break;
      }
      case "turn_start": {
        const said = held.get(d.prompt!);
        if (said) {
          held.delete(d.prompt!);
          start(said);
        }
        if (next === d.prompt) next = null;
        running = d.prompt ?? null;
        reading = items.findLast((i): i is QueuedItem => i.kind === "user" && i.seq === d.prompt);
        if (reading) reading.receipt = { got: reading.receipt?.got ?? reading.ts, taken: e.ts };
        break;
      }
      case "queue": {
        const said = held.get(d.seq);
        if (!said) break;
        if (d.action === "steered") said.steered = true;
        else if (d.action === "edited") {
          said.text = d.text ?? said.text;
          said.edited = true;
          if (!said.review && !said.sentBy && !said.fromHub) said.rule = ruleIn(said.text);
        } else {
          held.delete(d.seq);
          // Let go after the engine kept failing: it shows as sent, without an answer.
          if (d.action === "dropped") start(said);
        }
        break;
      }
      case "update": {
        // The engine's own JSON: read as it came.
        const d = e.data as Json;
        const u = d.sessionUpdate as string;
        // The first thing the agent says or does about a message is when it has read it.
        if (reading?.receipt && !reading.receipt.seen && /^(agent_|tool_call|plan)/.test(u)) reading.receipt.seen = e.ts;
        if (u === "user_message_chunk") {
          // Only what was copied in from a terminal; Hub's own prompts are `user_message`s.
          const text = d.content?.type === "text" ? (d.content.text as string) : "";
          const prev = last();
          if (prev && prev.kind === "user" && prev.terminal) prev.text += text;
          else {
            edits = new Map();
            items.push({ kind: "user", seq: e.seq, text, ts: e.ts, attachments: [], terminal: true });
          }
        } else if (u === "agent_message_chunk" || u === "agent_thought_chunk") {
          const picture = image(d.content, e.seq, 0);
          if (picture) {
            items.push(picture);
            break;
          }
          const text = d.content?.type === "text" ? (d.content.text as string) : "";
          const kind = u === "agent_message_chunk" ? "assistant" : "thought";
          const prev = last();
          if (prev && prev.kind === kind) prev.text += text;
          else items.push({ kind, text });
        } else if (u === "tool_call" || u === "tool_call_update") {
          // Streamed command output is most of a long thread's log: it only grows the output.
          if (outputOnly(d) && tools.has(d.toolCallId)) {
            addOutput(tools.get(d.toolCallId)!, d._meta);
            break;
          }
          calls.set(d.toolCallId, { ...calls.get(d.toolCallId), ...defined(d) });
          let tool = tools.get(d.toolCallId);
          if (!tool) {
            tool = { kind: "tool", id: d.toolCallId, title: d.title ?? "Tool", status: d.status ?? "pending" };
            tools.set(tool.id, tool);
            items.push(tool);
          }
          if (d.title) tool.title = d.title;
          if (d.status) tool.status = d.status;
          if (d.kind) tool.toolKind = d.kind;
          tool.since ??= e.ts;
          if (d.status === "completed" || d.status === "failed") tool.until ??= e.ts;
          if (d._meta?.terminal_info || (d.content ?? []).some((c: Json) => c.type === "terminal")) tool.terminal = true;
          const command = commandOf(d.rawInput);
          if (command) tool.command = command;
          addOutput(tool, d._meta);
          const diffs = (d.content ?? []).filter((c: Json) => c.type === "diff" && c.path);
          if (diffs.length)
            edits.set(tool.id, diffs.map((c: Json) => ({ path: c.path, old: c.oldText ?? "", new: c.newText ?? "" })));
          const detail = toolDetail(d);
          if (detail) tool.detail = detail;
          // page_check's screenshot is the agent looking at its own page before it shows it: the
          // page itself is in the reply, so the picture would only be a second copy of it.
          if (!shownImages.has(tool.id) && !/page_check/i.test(tool.title)) {
            const found = (d.content ?? []).flatMap((c: Json) => (c.type === "content" && c.content?.type === "image" ? [c.content] : [])).map((c: Json, n: number) => image(c, e.seq, n, d.content)).filter(Boolean);
            if (found.length) shownImages.add(tool.id);
            items.push(...found);
          }
        } else if (u === "notice") {
          const title = String(d.title ?? "").trim();
          // The same notice again (Codex repeats its warnings each turn) is said once.
          const said = items.some((i) => i.kind === "notice" && i.title === title);
          if (title && !said) items.push({ kind: "notice", severity: d.severity ?? "info", title, description: d.description ?? undefined, ts: e.ts });
        } else if (u === "plan") {
          // A plan is updated in place; a message of yours starts a new one (it belongs to its own turn).
          const last = items.findLast((i) => i.kind === "plan" || i.kind === "user");
          const prev = last?.kind === "plan" ? last : null;
          const entries = (d.entries ?? []).map((p: Json) => ({ content: p.content, status: p.status }));
          const marks = prev?.marks ?? [];
          entries.forEach((p: { status: string }, n: number) => {
            const m = (marks[n] ??= {});
            if (p.status !== "pending" && m.from == null) m.from = e.ts;
            if (p.status === "completed" && m.to == null) m.to = e.ts;
          });
          if (prev) {
            prev.entries = entries;
            prev.marks = marks;
          } else items.push({ kind: "plan", entries, marks });
        }
        break;
      }
      case "permission_request": {
        // `tool_call` and `options` are the engine's own JSON.
        const asked = (d.tool_call ?? {}) as Json;
        const call = { ...calls.get(asked.toolCallId), ...defined(asked) };
        const command = typeof call.rawInput?.command === "string" ? (call.rawInput.command as string) : undefined;
        const p: PermissionItem = {
          kind: "permission",
          id: d.id,
          title: asked.title ?? "The agent wants to continue",
          options: (d.options ?? []) as PermissionItem["options"],
          resolved: false,
          risk: toolRisk(call),
          command,
        };
        permissions.set(p.id, p);
        items.push(p);
        break;
      }
      case "permission_resolved": {
        const p = permissions.get(d.id);
        if (p) {
          p.resolved = true;
          p.answer = d.option_id;
        }
        break;
      }
      case "permission_auto":
        // Allowed ones just run; a refusal says why the agent heard no.
        if (!d.allowed) items.push({ kind: "refused", title: (d.tool_call as Json | null)?.title ?? "A tool call", why: d.why ?? "" });
        break;
      case "rule_saved": {
        const said = items.find((i) => i.kind === "user" && i.seq === d.seq);
        if (said && said.kind === "user") said.saved = d.text;
        break;
      }
      case "checkpoint": {
        const said = items.findLast((i) => i.kind === "user" && i.seq === d.seq);
        if (said && said.kind === "user") said.checkpoint = d.commit;
        break;
      }
      case "restored": {
        const said = items.find((i) => i.kind === "user" && i.seq === d.seq);
        items.push({
          kind: "restored",
          seq: d.seq ?? null,
          text: said && said.kind === "user" ? said.text : undefined,
          files: d.files ?? 0,
          commits: d.commits ?? 0,
          keptCommits: d.kept_commits ?? undefined,
          safety: d.safety,
          ts: e.ts,
        });
        break;
      }
      case "terminal":
        if (d.state === "taken" || d.state === "imported")
          items.push({ kind: "terminal", state: d.state, turns: d.turns ?? 0, carryOn: !!d.carry_on, ts: e.ts });
        break;
      case "quota_wait": {
        const wait = items.findLast((i) => i.kind === "wait" && i.seq === d.wait);
        if (wait?.kind === "wait") Object.assign(wait, { state: d.state, by: d.by ?? undefined, to: d.to ?? undefined, from: d.from ?? undefined, switchedAt: d.state === "switched" ? e.ts : undefined });
        else {
          // The engine's error says the same; the wait line takes its place.
          if (last()?.kind === "error") items.pop();
          items.push({
            kind: "wait",
            seq: e.seq,
            state: known(d.state),
            at: d.at ?? undefined,
            known: d.known ?? false,
            limit: d.limit ?? undefined,
            message: d.message ?? undefined,
            offer: d.offer ?? undefined,
            tries: d.tries ?? 0,
          });
        }
        break;
      }
      case "restart":
        reading = undefined;
        running = next = null;
        // The reply so far ends here; what the agent says after carrying on is a new one.
        flush();
        edits = new Map();
        items.push({ kind: "restart", by: known(d.by ?? "crash"), state: known(d.state), why: known(d.why ?? undefined), queued: (d.queued ?? []).length, ts: e.ts });
        break;
      case "handed_off":
        items.push({ kind: "handed_off", tool: d.tool ?? "", title: d.title ?? "", ok: !!d.ok, output: d.output ?? "", ts: e.ts });
        // An offer to the same tool is answered once the work went over.
        for (const o of items) if (o.kind === "handoff_offer" && o.folder === d.folder) d.ok ? (o.done = true) : (o.failedAt = e.ts);
        break;
      case "handoff_offer":
        if (d.folder)
          items.push({
            kind: "handoff_offer",
            tool: d.tool ?? "",
            folder: d.folder,
            label: d.label ?? `Hand to ${d.tool ?? "the tool"}`,
            about: d.about ?? null,
            canBuild: !!d.can_build,
            why: d.why ?? "",
            done: false,
            ts: e.ts,
          });
        break;
      case "worker_run": {
        const { lines, ...run } = d as unknown as WorkerRun & { lines?: WorkerLine[] };
        const card = items.find((i): i is Extract<Item, { kind: "worker_run" }> => i.kind === "worker_run" && i.run.run === run.run && i.run.folder === run.folder);
        if (card) {
          card.run = run;
          card.lines = card.lines.concat(lines ?? []);
          card.ts = e.ts;
        } else items.push({ kind: "worker_run", run, lines: lines ?? [], ts: e.ts });
        break;
      }
      case "handoff_result":
        items.push({
          kind: "handoff_result",
          tool: d.tool ?? "",
          title: d.title ?? "",
          state: d.state ?? "done",
          proven: d.proven ?? null,
          cost: d.cost ?? null,
          said: d.said ?? undefined,
          link: d.link ?? undefined,
          ts: e.ts,
        });
        break;
      case "sign_in_lost":
        // The engine's error says the same; this line takes its place.
        if (last()?.kind === "error") items.pop();
        items.push({
          kind: "sign_in",
          engine: d.engine ?? "",
          label: d.label ?? d.engine ?? "The engine",
          machine: d.machine ?? "",
          message: d.message ?? "",
          kept: !!d.kept,
          canKeep: d.can_keep !== false,
          keychain: !!d.keychain,
          ts: e.ts,
        });
        break;
      case "paused":
        flush();
        edits = new Map();
        items.push({ kind: "paused", by: d.by ?? "", ts: e.ts });
        break;
      case "rewind": {
        // The conversation goes on from before message `from`; what followed folds into one line.
        flush();
        const at = items.findLastIndex((i) => i.kind === "user" && i.seq === d.from);
        const cut = at < 0 ? [] : items.splice(at);
        for (const said of held.values()) if (said.seq >= d.from!) held.delete(said.seq);
        reading = undefined;
        running = next = null;
        edits = new Map();
        // The rollback of the files goes with the edit, not with what was left out: it stays in
        // sight so it can be undone.
        const restored: Item[] = [];
        while (cut.at(-1)?.kind === "restored") restored.unshift(cut.pop()!);
        items.push({ kind: "rewound", from: d.from!, files: !!d.files, ts: e.ts, cut }, ...restored);
        break;
      }
      case "moved":
        flush();
        edits = new Map();
        items.push({ kind: "moved", from: d.from ?? "", folder: d.folder ?? "", dirty: d.dirty ?? 0, stopped: !!d.stopped, ts: e.ts });
        break;
      case "moved_away":
        flush();
        items.push({ kind: "moved_away", to: d.to ?? "", ts: e.ts });
        break;
      case "engine_switch":
        items.push({ kind: "switch", from: d.from, to: d.to, ts: e.ts, carried: carriedBy(items) });
        break;
      case "quota_switch":
        items.push({ kind: "quota", limit: d.limit, percent: d.percent, to: d.to });
        break;
      case "auto":
        if (typeof d.on === "boolean") items.push({ kind: "auto", on: d.on, by: d.by ?? "you" });
        else items.push({ kind: "auto", ask: d.ask ?? "work", engine: d.engine ?? "", why: d.why ?? "", to: d.to ?? undefined, toName: plain(d.to_name), fromName: plain(d.from_name) });
        break;
      case "design": {
        if (typeof d.on === "boolean") items.push({ kind: "design", on: d.on, by: d.by ?? "you", ts: e.ts });
        else if (Array.isArray(d.strayed)) items.push({ kind: "design_strayed", files: d.strayed });
        else if (typeof d.review === "string") {
          const r: DesignReviewItem = {
            kind: "design_review",
            id: d.review,
            state: known(d.state ?? "running"),
            doc: d.doc ?? "",
            checks: (d.checks ?? []).map((c: Json) => ({
              name: c.name ?? "",
              title: c.title ?? "",
              model: c.model ?? undefined,
              state: c.state ?? "running",
              verdict: c.verdict || undefined,
              text: c.text ?? undefined,
              error: c.error ?? undefined,
            })),
            verdict: d.verdict ?? undefined,
            ready: d.ready ?? undefined,
            sent: d.sent ?? undefined,
            by: d.by ?? "you",
            error: d.error ?? undefined,
          };
          const seen = designs.get(r.id);
          if (seen) Object.assign(seen, r);
          else {
            designs.set(r.id, r);
            items.push(r);
          }
        }
        break;
      }
      case "review": {
        const seen = reviews.get(d.id);
        const r: ReviewItem = {
          kind: "review",
          id: d.id,
          state: known(d.state),
          model: d.model ?? "",
          files: d.files ?? [],
          seq: d.seq ?? 0,
          by: d.by ?? "you",
          reason: d.reason ?? undefined,
          parts: (d.chunks ?? []).map((c: Json) => ({
            title: c.title ?? "",
            files: c.files ?? [],
            why: c.why ?? "",
            state: c.state ?? "waiting",
            verdict: c.verdict || undefined,
            text: c.text ?? undefined,
            error: c.error ?? undefined,
          })),
          sent: d.sent ?? undefined,
          text: d.text ?? undefined,
          error: d.error ?? undefined,
        };
        if (seen) Object.assign(seen, r);
        else {
          reviews.set(r.id, r);
          items.push(r);
        }
        break;
      }
      case "turn_end": {
        lastActive = e.ts;
        if (reading?.receipt) Object.assign(reading.receipt, { done: e.ts, outcome: d.stop_reason === "cancelled" ? "stopped" : undefined });
        reading = undefined;
        running = next = null;
        const answered = answering && reviews.get(answering);
        if (answered) answered.fixed = true;
        answering = undefined;
        const designed = answeringDesign && designs.get(answeringDesign);
        if (designed) designed.fixed = true;
        answeringDesign = undefined;
        const files = changed(edits, tools);
        if (files.length) items.push({ kind: "changes", files, seq: e.seq });
        items.push({ kind: "turn", seq: e.seq, ts: e.ts, replies: offered });
        offered = undefined;
        edits = new Map();
        if (d.stop_reason && d.stop_reason !== "end_turn") items.push({ kind: "end", reason: String(d.stop_reason) });
        flush();
        break;
      }
      case "proof": {
        const proof: ProofState = { state: known(d.state), via: known(d.kind ?? undefined), command: d.command ?? undefined, why: d.why ?? undefined, hidden: d.hidden === true };
        const reply = items.find((i) => (i.kind === "changes" || i.kind === "proof") && (i.kind === "changes" ? i.seq : i.reply) === d.reply);
        if (reply?.kind === "changes") reply.proof = proof;
        else if (reply?.kind === "proof") Object.assign(reply, proof);
        // Files changed by commands rather than edits have no list of their own.
        else items.push({ kind: "proof", reply: d.reply!, ...proof });
        break;
      }
      case "docs": {
        const docs: DocsState = { state: known(d.state), sections: d.sections ?? [], why: d.why ?? undefined };
        const reply = items.find((i) => (i.kind === "changes" && i.seq === d.reply) || (i.kind === "proof" && i.reply === d.reply));
        if (reply?.kind === "changes" || reply?.kind === "proof") reply.docs = docs;
        break;
      }
      case "clash": {
        const prev = clashes.find((c) => c.other === d.other);
        if (prev) prev.paths.includes(d.path) || prev.paths.push(d.path);
        else clashes.push({ kind: "clash", other: d.other, otherTitle: d.other_title ?? "Another thread", paths: [d.path] });
        break;
      }
      case "focus": {
        // Right under the message it is about, so it neither waits for the reply nor cuts it in two.
        if (d.fits !== false) break;
        const item: Item = { kind: "focus", stage: d.stage ?? "", why: d.why ?? "", seq: e.seq };
        const first = items.findIndex((i) => i.kind === "user");
        if (first < 0) items.push(item);
        else items.splice(first + 1, 0, item);
        break;
      }
      case "reading": {
        // A new-task reading shows under the message it is about.
        if (d.feature === "topic" && d.state === "new") {
          const at = items.findIndex((i) => i.kind === "user" && i.seq === d.seq);
          if (at >= 0) items.splice(at + 1, 0, { kind: "topic", userSeq: d.seq!, text: (items[at] as Extract<Item, { kind: "user" }>).text, by: d.connection ?? "" });
          break;
        }
        // Only the focus reading shows, under the first message; a turn's reading marks the thread.
        if (d.feature !== "focus" || d.line == null) break;
        const item: Item = { kind: "aim", stage: d.stage ?? "", text: d.text ?? "", by: d.connection ?? "" };
        const first = items.findIndex((i) => i.kind === "user");
        if (first < 0) items.push(item);
        else items.splice(first + 1, 0, item);
        break;
      }
      case "leak":
        items.push({ kind: "leak", findings: d.findings ?? [], sent: !!d.sent });
        break;
      case "customs":
        items.push({ kind: "customs", found: d.found ?? [], sent: !!d.sent, declared: !!d.declared });
        break;
      case "output": {
        // What Hub read of a command's output while it ran, for engines that send it at the end.
        const tool = tools.get(d.tool_call_id);
        if (tool && (tool.live || tool.output == null)) {
          tool.live = true;
          addOutput(tool, { hub: true, terminal_output_delta: { data: d.data } });
        }
        break;
      }
      case "attempt":
        if (d.state === "picked" || d.state === "dropped")
          items.push({ kind: "attempt", state: d.state, files: d.files ?? 0, cwd: d.cwd ?? undefined, safety: d.safety ?? undefined, by: d.by ?? undefined });
        break;
      case "child": {
        const c: ChildItem = {
          kind: "child",
          id: d.id,
          title: d.title ?? "Side job",
          job: d.kind ?? "other",
          engine: d.engine ?? "",
          state: known(d.state ?? "working"),
          images: d.images ?? [],
          reply: d.reply ?? undefined,
          error: d.error ?? undefined,
          doneTs: d.state === "done" || d.state === "failed" ? e.ts : undefined,
          ts: e.ts,
        };
        const seen = children.get(c.id);
        if (seen) Object.assign(seen, { ...c, ts: seen.ts });
        else {
          children.set(c.id, c);
          items.push(c);
        }
        break;
      }
      case "task": {
        const t: TaskItem = {
          kind: "task",
          id: d.id ?? String(e.seq),
          task: d.task ?? "",
          name: d.name ?? "",
          engine: d.engine ?? "",
          model: d.model ?? undefined,
          state: known(d.state ?? "working"),
          verdict: d.verdict ?? undefined,
          reason: d.reason ?? undefined,
          ms: typeof d.ms === "number" ? d.ms : undefined,
          ts: e.ts,
        };
        const seen = tasks.get(t.id);
        if (seen) Object.assign(seen, { ...t, ts: seen.ts });
        else {
          tasks.set(t.id, t);
          items.push(t);
        }
        break;
      }
      case "desk_note":
        items.push({ kind: "desk", thread: d.thread, title: d.title || "A thread", state: known(d.state), text: d.text ?? "", permission: !!d.permission, ts: e.ts });
        break;
      case "replies": {
        const options = ((d.options ?? []) as Json[])
          .filter((o) => typeof o?.label === "string" && o.label.trim())
          .map((o) => ({ label: o.label as string, message: (o.message as string) || (o.label as string) }));
        if (options.length >= 2) offered = { question: d.question ?? "", options };
        break;
      }
      case "ask": {
        const options = ((d.options ?? []) as Json[])
          .filter((o) => typeof o?.label === "string" && o.label.trim())
          .map((o) => ({ label: o.label as string, message: (o.message as string) || (o.label as string) }));
        if (d.question && options.length >= 2) {
          items.push({ kind: "ask", id: String(d.id ?? e.seq), seq: e.seq, question: d.question, assumed: d.assumed ?? "", options, ts: e.ts });
        }
        break;
      }
      case "plugin_suggested":
        if (d.repo?.full_name) items.push({ kind: "plugin", repo: d.repo, why: d.why ?? "", ts: e.ts });
        break;
      case "page":
      {
        const earlier = d.replaces ? pages.get(d.replaces) : undefined;
        const family = earlier?.family ?? d.id;
        const kin = [...pages.values()].filter((p) => p.family === family);
        const page: PageItem = { kind: "page", id: d.id, title: d.title || "Page", height: d.height ?? undefined, ts: e.ts, family, version: kin.length + 1, of: kin.length + 1 };
        for (const p of kin) p.of = page.of;
        pages.set(d.id, page);
        items.push(page);
        break;
      }
      case "error":
        if (reading?.receipt && !reading.receipt.done) Object.assign(reading.receipt, { done: e.ts, outcome: "failed" });
        reading = undefined;
        running = next = null;
        {
          const message = d.message ?? "The engine failed";
          // An engine that cannot start fails again on each try: one card, counted.
          const before = last();
          if (before?.kind === "error" && before.message === message) {
            Object.assign(before, { seq: e.seq, times: before.times + 1, detail: d.detail ?? before.detail, prompt: d.prompt ?? before.prompt });
            before.seqs.push(e.seq);
          } else items.push({ kind: "error", seq: e.seq, seqs: [e.seq], times: 1, message, detail: d.detail ?? undefined, prompt: d.prompt ?? undefined });
        }
        break;
      case "reported": {
        const error = items.findLast((i): i is ErrorItem => i.kind === "error" && i.seqs.includes(d.about!));
        if (error)
          error.report = { problem: d.problem!, state: known(d.state), fix: d.fix ?? undefined, fixTitle: d.fix_title ?? undefined, text: d.text ?? undefined, pr: d.pr ?? undefined };
        break;
      }
    }
  }
  flush();
  // An answer is a message that starts with what it answers; the latest one counts.
  for (const ask of items) {
    if (ask.kind !== "ask") continue;
    const lead = askLead(ask);
    for (const m of [...items, ...held.values()]) if (m.kind === "user" && m.seq > ask.seq && m.text.startsWith(lead)) ask.answer = m.text.slice(lead.length);
  }
  // One being steered goes next, ahead of the rest.
  return { items, queue: [...held.values()].sort((a, b) => Number(!!b.steered) - Number(!!a.steered)), pending: changed(edits, tools) };
}

/** The transcript's line for a user message, as it was logged. */
function userItem(e: Event): QueuedItem {
  const d = e.data as Json;
  if (d.leak != null || d.customs != null || d.proof != null || d.docs != null || d.quota != null || d.restart != null || d.terminal != null || d.watch != null || d.child != null || d.routine != null || d.recipe != null || d.vakten != null || d.widget != null) {
    const fromHub =
      d.leak != null ? "leak" : d.customs != null ? "customs" : d.proof != null ? "proof" : d.docs != null ? "docs" : d.quota != null ? "quota" : d.terminal != null ? "terminal" : d.watch != null ? "watch" : d.child != null ? "child" : d.routine != null ? "routine" : d.recipe != null ? "recipe" : d.vakten != null ? "vakten" : d.widget != null ? "widget" : "restart";
    const recipe = d.recipe != null ? { name: d.recipe.name ?? "", by: d.recipe.by?.title ?? d.recipe.by?.agent } : undefined;
    return { kind: "user", seq: e.seq, text: d.text ?? "", ts: e.ts, attachments: [], fromHub, watch: d.watch?.number, routine: d.routine?.name, recipe, widget: d.widget != null ? { ask: d.widget.ask ?? "", change: !!d.widget.change } : undefined };
  }
  return {
    kind: "user",
    seq: e.seq,
    text: d.text ?? "",
    ts: e.ts,
    attachments: d.attachments ?? [],
    original: d.original ?? undefined,
    rule: d.review || d.design || d.from ? undefined : ruleIn(d.text ?? ""),
    review: d.review ?? undefined,
    designReview: d.design ?? undefined,
    sentBy: d.from ? { thread: d.from.thread ?? undefined, name: d.from.title ?? d.from.agent ?? "an agent" } : undefined,
  };
}

/** Where the thread stands as one of several attempts: how it started, and whether one was picked. */
export function attemptOf(events: Event[]): (AttemptStart & { done: boolean }) | null {
  let start: AttemptStart | null = null;
  let done = false;
  for (const e of events) {
    const d = payload(e, "attempt");
    if (!d) continue;
    if (typeof d.n === "number") start = { group: d.group, n: d.n, of: d.of!, threads: d.threads ?? [], cwd: d.cwd ?? "" };
    else if (d.state) done = true;
  }
  return start && { ...start, done };
}

/** Edits of tool calls that did not fail, grouped by file in the order they were first touched. */
function changed(edits: Map<string, { path: string; old: string; new: string }[]>, tools: Map<string, ToolItem>) {
  const files = new Map<string, FileChange>();
  for (const [id, list] of edits) {
    if (tools.get(id)?.status === "failed") continue;
    for (const e of list) {
      const file = files.get(e.path) ?? { path: e.path, edits: [] };
      file.edits.push({ old: e.old, new: e.new });
      files.set(e.path, file);
    }
  }
  return [...files.values()];
}


/** What a switch hands over, from the items so far. A part that was rewound is not among them,
 *  as it is not in the conversation Hub hands over (`conversation_log`). */
function carriedBy(items: Item[]): Carried {
  let messages = 0;
  let tools = 0;
  let files = 0;
  for (const i of items) {
    if (i.kind === "user") {
      messages++;
      files += i.attachments.length;
    } else if (i.kind === "assistant") messages++;
    else if (i.kind === "tool") tools++;
  }
  return { messages, tools, files };
}
