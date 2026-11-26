// What each room reads right now, shared by the rail and the sidebar header, so the two never
// disagree. Inbox: what waits for you, else what runs. System: only what is broken or left over.
import { threadProject } from "./format";
import { hub, waitsOnYou } from "./state.svelte";

export type Reading = { text: string; tone: "" | "amber" | "green" | "red" };

/** The names the rooms go by. The code calls them work and control. */
export const ROOM_NAMES = { work: "Inbox", control: "System" } as const;

export function inboxReading(): Reading {
  const needs = hub.active.filter(waitsOnYou).length;
  if (hub.halt) return { text: "All paused", tone: "amber" };
  if (needs) return { text: `${needs} need you`, tone: "amber" };
  if (hub.running.length) return { text: `${hub.running.length} working`, tone: "green" };
  return { text: "Quiet", tone: "" };
}

export function systemReading(): Reading {
  const offline = hub.hosts.find((h) => !h.local && h.state === "offline" && !!h.error);
  if (hub.update?.state === "failed") return { text: "Update failed", tone: "red" };
  if (offline) return { text: `${offline.name} offline`, tone: "red" };
  if (hub.strays.length) return { text: `${hub.strays.length} left on`, tone: "amber" };
  return { text: "All good", tone: "" };
}

/** What is broken, for the red dot on System's icon; null when nothing is. */
export function systemTrouble(): string | null {
  const r = systemReading();
  return r.tone === "red" ? r.text : null;
}

/** One thing the sidebar's bell lists: something that wants you now, and where it opens. */
export type NowItem = { kind: "thread" | "machine" | "problem"; tone: "amber" | "red"; title: string; detail: string; open: () => void };

/** What wants you now, worst first: failed and waiting threads, machines that dropped off,
 *  problems waiting on you. Empty when all is calm, and then the bell is not drawn. */
export function nowItems(): NowItem[] {
  const items: NowItem[] = [];
  for (const t of hub.active.filter(waitsOnYou)) {
    const failed = t.status === "error";
    items.push({
      kind: "thread",
      tone: failed ? "red" : "amber",
      title: t.title,
      detail: `${hub.projectName(threadProject(t), t.host && t.host !== hub.localHost ? t.host : null)} · ${failed ? "failed" : "waiting on you"}`,
      open: () => hub.open(t.id),
    });
  }
  for (const h of hub.hosts.filter((h) => !h.local && h.state === "offline" && !!h.error)) {
    items.push({ kind: "machine", tone: "red", title: `${h.name} is offline`, detail: "machine", open: () => hub.openMachine(h.name) });
  }
  const problems = hub.problemsNeedYou.length;
  if (problems) {
    items.push({
      kind: "problem",
      tone: "red",
      title: `${problems} ${problems === 1 ? "problem needs" : "problems need"} you`,
      detail: "Problems",
      open: () => {
        hub.view = "problems";
        hub.selected = null;
      },
    });
  }
  return items.sort((a, b) => (a.tone === b.tone ? 0 : a.tone === "red" ? -1 : 1));
}
