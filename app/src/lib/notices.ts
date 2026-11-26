// What Hub tells you on this computer, and when it keeps quiet. The window decides (`announce` in
// state.svelte.ts calls `runNotices`): the three switches and your own lines under Settings >
// Notifications say what may come through, and the quiet rules say when it may not. Nothing is on
// until you switch it on. A thread that is held back is still in the inbox; nothing is lost.
import { quiet } from "./quiet";
import type { Thread } from "./bindings/Thread";
import { chime } from "./sound";
import { attention, notify, toastCheck } from "./native";
import { idleMs } from "./presence";
import { threadProject } from "./format";
import { prefs, type NoticeLine, type NoticeSort } from "./prefs.svelte";

export const SORTS: { id: NoticeSort; label: string; hint: string; sentence: string; text: string }[] = [
  { id: "waiting", label: "Needs you", hint: "a question or an approval", sentence: "a thread needs me", text: "Needs your answer" },
  { id: "failed", label: "Failed", hint: "a thread or its engine stopped", sentence: "a thread fails", text: "The engine failed" },
  { id: "done", label: "Done", hint: "a turn finished", sentence: "a thread is done", text: "Done" },
];

export const HOWS: { value: NoticeLine["how"]; label: string }[] = [
  { value: "notice", label: "a Windows notice" },
  { value: "flash", label: "a taskbar flash" },
  { value: "sound", label: "a sound" },
];

export const AFTERS: { value: number; label: string }[] = [
  { value: 0, label: "at once" },
  { value: 5, label: "after 5 minutes" },
  { value: 15, label: "after 15 minutes" },
  { value: 60, label: "after an hour" },
];

/** What a change of status is worth a notice, if anything. */
export function sortOf(before: Thread, after: Thread): NoticeSort | null {
  if (after.status === "waiting_on_you") return "waiting";
  if (after.status === "error") return "failed";
  if (before.status === "running" && after.status === "idle") return "done";
  return null;
}

const SWITCH: Record<NoticeSort, () => boolean> = {
  waiting: () => prefs.notifyWaiting,
  failed: () => prefs.notifyFailed,
  done: () => prefs.notifyDone,
};

export const sortOn = (sort: NoticeSort) => SWITCH[sort]();

/** How many things may come through now: the switches that are on and your own lines. */
export function comingThrough(): number {
  return SORTS.filter((s) => sortOn(s.id)).length + prefs.noticeLines.length;
}

type Outcome = { notice: boolean; flash: boolean; sound: boolean };

const inProject = (t: Thread, project: string | null) => project === null || threadProject(t) === project;

/** What each rule that matches says to do, with how many minutes the thread must stay as it is first. */
function matches(t: Thread, sort: NoticeSort): { after: number; out: Outcome }[] {
  const found: { after: number; out: Outcome }[] = [];
  if (sortOn(sort) && inProject(t, prefs.noticeScope[sort])) found.push({ after: 0, out: { notice: true, flash: prefs.flashTaskbar, sound: true } });
  for (const l of prefs.noticeLines) {
    if (l.sort !== sort || !inProject(t, l.project)) continue;
    found.push({ after: l.after, out: { notice: l.how === "notice", flash: l.how === "flash", sound: l.how === "sound" || l.how === "notice" } });
  }
  return found;
}

/** Why Hub keeps quiet now about this sort, or null when it may speak. */
export function quietWhy(sort: NoticeSort, now = new Date()): "paused" | "hours" | "here" | null {
  const q = prefs.noticeQuiet;
  if (now.getTime() < q.pausedUntil) return "paused";
  if (q.hours && q.from !== q.to) {
    const h = now.getHours();
    const inside = q.from < q.to ? h >= q.from && h < q.to : h >= q.from || h < q.to;
    if (inside && (sort !== "waiting" || q.strict)) return "hours";
  }
  if (q.atPc && document.hasFocus() && idleMs() < 3 * 60_000) return "here";
  return null;
}

const timers = new Map<string, ReturnType<typeof setTimeout>[]>();

function speak(t: Thread, sort: NoticeSort, out: Outcome) {
  if (quietWhy(sort)) return;
  const text = SORTS.find((s) => s.id === sort)!.text;
  // The quiet rule already said whether Hub being in front matters, so a notice is shown either way.
  if (out.notice) void quiet(notify(t.title, text, true), "notify");
  if (out.flash) quiet(attention(), "attention");
  if (out.sound) chime(prefs.sound === "off" && !out.notice ? "soft" : prefs.sound);
}

/** Tells you about a thread's change of status, as the switches, your lines and the quiet rules allow.
 *  `still` says whether the thread is still as it was, for a line that waits. */
export function runNotices(t: Thread, sort: NoticeSort, still: () => boolean) {
  for (const id of timers.get(t.id) ?? []) clearTimeout(id);
  timers.delete(t.id);
  const found = matches(t, sort);
  const now = found.filter((f) => f.after === 0).reduce<Outcome>((all, f) => ({ notice: all.notice || f.out.notice, flash: all.flash || f.out.flash, sound: all.sound || f.out.sound }), { notice: false, flash: false, sound: false });
  if (now.notice || now.flash || now.sound) speak(t, sort, now);
  const later = found.filter((f) => f.after > 0).map((f) => setTimeout(() => still() && speak(t, sort, f.out), f.after * 60_000));
  if (later.length) timers.set(t.id, later);
}

/** Shows a notice now, to see how Windows shows it. Says what happened. */
export async function testNotice(): Promise<string> {
  try {
    const blocked = await toastCheck();
    const shown = await notify("Hub · Test notice", "If you can read this, Windows lets Hub notify you.", true);
    if (shown) chime(prefs.sound);
    if (shown && blocked) return `Sent, but you will probably not see it: ${blocked}`;
    return shown
      ? "Sent. Look in the corner of the screen, and in Windows' Action Center. If you only hear the sound, Windows is hiding it."
      : "Windows did not allow Hub to show a notice. Allow Hub under Windows Settings > System > Notifications.";
  } catch (err) {
    return `Could not send it: ${err instanceof Error ? err.message : String(err)}`;
  }
}
