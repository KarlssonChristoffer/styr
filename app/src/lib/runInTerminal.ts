// "Run in terminal" on a command in a reply: what is typed, and where it can run. It is only ever
// typed, never run: the person reads it and presses Enter (RunTerminal.svelte).
import { api } from "./api";
import { hub } from "./state.svelte";
import type { Thread } from "./bindings/Thread";

/** The text of a code block as one command to type: prompt marks dropped, a line ending in `\` joined to the next. */
export function commandText(code: string): string {
  const lines = code.replace(/\r\n?/g, "\n").trim().split("\n");
  const prompted = lines.filter((l) => l.trim()).every((l) => /^[$>]\s+/.test(l));
  const bare = lines.map((l) => (prompted ? l.replace(/^[$>]\s+/, "") : l).replace(/\s+$/, ""));
  return bare.join("\n").replace(/\\\n[ \t]*/g, "").trim();
}

/**
 * What to type into the shell. More than one line goes in as a bracketed paste, so the shell
 * keeps it as one piece instead of running each line at its newline. A single line is typed as it is.
 */
export function typedCommand(command: string): string {
  return command.includes("\n") ? `\x1b[200~${command.replace(/\n/g, "\r")}\x1b[201~` : command;
}

/** A `cd` into the thread's folder, sent before the command when the terminal is on the thread's own machine. */
export const cdLine = (cwd: string) => `cd "${cwd}"\r`;

export type RunTarget = {
  /** The machine's name for calls (`machineTerminal`). */
  name: string;
  /** Folder to start in: the thread's own, on its own machine. */
  cwd: string | null;
  here: boolean;
};

/** The machines a command can be typed on: the thread's own first, then the others that are online. */
export function targetsFor(t: Thread): RunTarget[] {
  const own = hub.hostOf(t);
  const rest = hub.hosts.filter((h) => h.name !== own && h.state === "online").map((h): RunTarget => ({ name: h.name, cwd: null, here: false }));
  return [{ name: own, cwd: t.cwd, here: true }, ...rest];
}

/** Whether the machine is Windows, from its own page's facts; null when it does not say. */
export async function isWindows(name: string): Promise<boolean | null> {
  try {
    const about = await api.hostAbout(name === hub.localHost ? null : name);
    return /windows/i.test(about.os);
  } catch {
    return null;
  }
}
