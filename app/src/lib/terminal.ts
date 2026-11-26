// Taking a thread over in a terminal: the core stops its engine and `hubd attach` runs the
// engine's own CLI on the same session there (see crates/hub-core/src/terminal.rs).
import { api } from "./api";
import type { Thread } from "./bindings/Thread";
import { hub } from "./state.svelte";

/** What `hubd attach` and tmux call a thread: the random end of its id. */
export const shortId = (id: string) => id.replace(/[^0-9a-f]/gi, "").slice(-8);

/** A command typed here reaches a thread on another machine over the SSH alias Hub uses. */
function viaHost(t: Thread, command: string) {
  if (!t.host) return command;
  const address = hub.hostInfo(t.host)?.address ?? "";
  return address.startsWith("ssh ") ? `ssh -t ${address.slice(4)} '${command}'` : command;
}

export const attachCommand = (t: Thread) => viaHost(t, `hubd attach ${shortId(t.id)}`);

/**
 * Takes the thread over in Hub's own terminal: this computer runs `hubd attach <thread> --if-idle`
 * in a terminal inside Hub (over SSH for a thread on another machine): it shows the agent working
 * without stopping it, and takes over at once only when the thread is idle, otherwise on Enter. The thread's side
 * column opens at it. A terminal already open for the thread is shown again. Says what happened,
 * in a few words, when there is more to say than the terminal showing.
 */
export async function takeOver(t: Thread): Promise<string | null> {
  const known = hub.takeovers[t.id];
  const open = await api.terminals().then(
    (all) => all.find((x) => x.what === "attach" && x.thread === t.id && x.exit === null && (!known || x.id === known)),
    () => undefined,
  );
  if (open) {
    hub.showTakeover(t.id, open.id);
    return null;
  }
  try {
    const term = await api.machineTerminal(hub.hostOf(t), { what: "attach", engine: null, thread: t.id, cols: 80, rows: 30 });
    hub.showTakeover(t.id, term.id);
    return null;
  } catch (e) {
    // A machine Hub reaches without SSH: the old way, a window or a command to paste.
    if (!t.host || !/no SSH alias/.test(String(e))) throw e;
    return takeOverOutside(t);
  }
}

/** Takes the thread over in a terminal outside Hub: a window on Windows, a tmux session
 * elsewhere, whose attach command is copied. */
export async function takeOverOutside(t: Thread): Promise<string> {
  const opened = await api.openTerminal(t.id);
  if (opened.opened) return "Opened in a terminal";
  await navigator.clipboard.writeText(viaHost(t, opened.attach));
  return opened.session ? "Copied the tmux attach command" : "Copied, paste in a terminal";
}

/**
 * Brings the thread back to Hub. A terminal inside Hub that has it is closed first, which ends
 * the CLI in it, so it does not go on writing to the same session.
 */
export async function takeBack(t: Thread) {
  await hub.closeTakeover(t.id);
  await api.handBack(t.id);
}
