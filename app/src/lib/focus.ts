// Focus mode: one project shows in the lists and the others' notifications wait (`prefs.focusOn`). The calls live on `hub`; these hold what they do.
import { api } from "./api";
import type { Thread } from "./bindings/Thread";
import { prefs, setPref } from "./prefs.svelte";
import { folderName, threadProject } from "./format";
import type { HubState } from "./state.svelte";

/** Whether a thread belongs to the project in focus; every thread does outside focus mode. */
export function inFocus(hub: HubState, t: Thread): boolean {
  const f = prefs.focusOn;
  return !f || (threadProject(t) === f.cwd && (t.host && t.host !== hub.localHost ? t.host : null) === f.host);
}

/** Focus mode: only this project shows in the lists, and the others' notifications wait. */
export function enterFocus(hub: HubState, cwd: string, host: string | null = null) {
  const remote = host && host !== hub.localHost ? host : null;
  const name = hub.projectName(cwd, remote);
  const already = prefs.focusOn;
  setPref("focusOn", { cwd, host: remote, name, since: already?.cwd === cwd && already.host === remote ? already.since : Date.now() });
  hub.workScope = null;
  hub.released = null;
  // A thread of another project in front of you is what focus mode is meant to put away.
  if (hub.view === "thread" && hub.current && !hub.inFocus(hub.current)) hub.openProject(cwd, remote);
}

/** Goes into a project: focus mode on it, with its desk open, the thread you steer the
 *  project's other threads from. The desk is made the first time. */
export async function goIn(hub: HubState, cwd: string, host: string | null = null) {
  const remote = host && host !== hub.localHost ? host : null;
  const before = prefs.focusOn;
  hub.enterFocus(cwd, remote);
  try {
    const desk = await api.openDesk(cwd, remote);
    if (!hub.desks.includes(desk.id)) hub.desks = [...hub.desks, desk.id];
    hub.upsert(desk);
    hub.open(desk.id);
  } catch (e) {
    // No desk (a machine whose hubd has none yet, say): you are not in, so focus goes back to
    // what it was instead of quietly hiding every other project.
    setPref("focusOn", before);
    hub.fail(e);
  }
}

/** The desk of the project in focus, when it has one. */
export function focusDesk(hub: HubState): Thread | null {
  const f = prefs.focusOn;
  if (!f) return null;
  return hub.threads.find((t) => hub.isDesk(t) && hub.inFocus(t)) ?? null;
}

/** Leaves focus mode and shows what the other projects did meanwhile. */
export function leaveFocus(hub: HubState) {
  const f = prefs.focusOn;
  if (!f) return;
  const held = prefs.held;
  setPref("focusOn", null);
  setPref("held", []);
  hub.released = held.length ? { name: f.name, since: f.since, until: Date.now(), held } : null;
}

/** The name a project goes by: the one it was added with, else its folder's. */
export function projectName(hub: HubState, cwd: string, host: string | null = null): string {
  return (!host && hub.projects.find((p) => p.path === cwd)?.name) || folderName(cwd);
}
