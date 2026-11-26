// Reading a tool call off the event log: its command, its output and the pictures it showed.
import type { Item, ImageItem, ToolItem } from "./transcriptItems";

type Json = Record<string, any>;

/** A tool call update that only carries a piece of a command's output. */
export function outputOnly(d: Json): boolean {
  return (d._meta?.terminal_output_delta != null || d._meta?.terminal_output != null) && d.status == null && d.title == null && d.kind == null && d.content == null && d.rawInput == null;
}

/** How much of a command's output the thread keeps: the end of it, as a terminal's scrollback. */
const KEEP = 256 * 1024;

/** Adds a chunk of output (appended) or the whole of it (replacing), and the exit code. */
export function addOutput(tool: ToolItem, meta: Json | undefined) {
  if (!meta) return;
  const delta = meta.terminal_output_delta?.data;
  const whole = meta.terminal_output?.data;
  if (typeof delta === "string" || typeof whole === "string") {
    tool.terminal = true;
    if (tool.live && !meta.hub) {
      // The engine's own output takes the place of what Hub read while the command ran.
      tool.live = false;
      tool.output = undefined;
      tool.cut = false;
    }
    tool.output = typeof whole === "string" ? whole : (tool.output ?? "") + delta;
    if (tool.output.length > 2 * KEEP) {
      tool.output = tool.output.slice(-KEEP);
      tool.cut = true;
    }
  }
  if (meta.terminal_exit) {
    tool.terminal = true;
    tool.exit = typeof meta.terminal_exit.exit_code === "number" ? meta.terminal_exit.exit_code : null;
  }
}

/** The command line of a tool call's input: Claude's `command`, or Codex's argument list. */
export function commandOf(input: Json | undefined): string | undefined {
  const c = input?.command ?? input?.cmd;
  if (typeof c === "string") return c;
  if (Array.isArray(c) && c.every((a) => typeof a === "string")) return c.join(" ");
  return undefined;
}

/** The commands a thread ran in a terminal, oldest first. */
export function commands(items: Item[]): ToolItem[] {
  return items.filter((i): i is ToolItem => i.kind === "tool" && !!i.terminal);
}

export function defined(o: Json): Json {
  return Object.fromEntries(Object.entries(o).filter(([, v]) => v !== undefined && v !== null));
}

/** An ACP image block as an item, with the revised prompt Codex puts beside it. */
export function image(c: Json | undefined, seq: number, n: number, siblings: Json[] = []): ImageItem | undefined {
  if (c?.type !== "image" || !c.data) return undefined;
  const note = siblings.map((s: Json) => s.content).find((b: Json) => b?.type === "text" && b.text?.startsWith("Revised prompt: "));
  return defined({
    kind: "image",
    id: `${seq}:${n}`,
    src: `data:${c.mimeType ?? "image/png"};base64,${c.data}`,
    path: typeof c.uri === "string" ? c.uri.replace(/^file:\/\//, "") : undefined,
    prompt: note?.text.slice("Revised prompt: ".length),
  }) as ImageItem;
}

export function toolDetail(d: Json): string | undefined {
  const parts: string[] = [];
  for (const c of d.content ?? []) {
    if (c.type === "content" && c.content?.type === "text") parts.push(c.content.text);
    else if (c.type === "diff") parts.push(`${c.path}\n${c.newText ?? ""}`);
  }
  if (!parts.length && d.rawInput) {
    const input = d.rawInput as Json;
    const line = input.command ?? input.file_path ?? input.path ?? input.pattern;
    if (typeof line === "string") parts.push(line);
  }
  return parts.join("\n").trim() || undefined;
}
