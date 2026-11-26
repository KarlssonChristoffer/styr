// Small pure formatters and path/thread predicates shared by the whole UI (folder names, ago, clip, labels).
import type { Project } from "./bindings/Project";
import type { Thread } from "./bindings/Thread";

export function folderName(path: string): string {
  const folder = projectFolder(path);
  if (isScratch(folder)) return "Scratch";
  if (isChats(folder)) return "Chats";
  const parts = folder.split(/[\\/]+/).filter(Boolean);
  return parts.at(-1) ?? path;
}

/** A "No project" thread's folder is `<projects root>/scratch/<date-time>` (FolderPicker's
 *  `scratchFolder`). They all belong to one Scratch project per machine, the `scratch` folder,
 *  so quick one-off threads never show up as projects named by a time stamp. */
const SCRATCH_RUN = /^(.*[\\/]scratch)[\\/]\d{4}-\d{2}-\d{2}-\d{6}(?:-\d+)?(?:[\\/].*)?$/;

/** A folder "No project" made for one thread. */
export function isScratchRun(path: string): boolean {
  return SCRATCH_RUN.test(path);
}

/** Home and scratch history cannot implicitly select the next coding workspace. */
export function codingFolderDefault(path: string): boolean {
  return !!path.trim() && !isScratchRun(path) && !isChatThread({ cwd: path }) && !["desk", "platschef"].includes(folderName(path).toLowerCase());
}

/** The Scratch project: the folder "No project" threads get their own folders in. */
export function isScratch(path: string): boolean {
  return /[\\/]scratch[\\/]?$/.test(path);
}

/** The folder the iPhone app's everyday chats get their own folders in
 *  (`<projects root>/chats/<date-time>`, `POST /api/phone/threads` with `chat`). */
/** The folder a chat's "Starting" stand-in shows before the core has made its real folder, so it
 *  already counts as a chat (Chat stays on screen, Inbox does not get it). */
export const STARTING_CHAT = "/chats/2000-01-01-000000";
const CHAT_RUN = /^(.*[\\/]chats)[\\/]\d{4}-\d{2}-\d{2}-\d{6}(?:-\d+)?(?:[\\/].*)?$/;

export function isChats(path: string): boolean {
  return /[\\/]chats[\\/]?$/.test(path);
}

/** A thread of the iPhone's Chat world: one that runs in a folder of its own under `chats/`. The
 *  path alone tells it apart, on every surface. */
export function isChatThread(t: { cwd: string }): boolean {
  return isChats(projectFolder(t.cwd));
}

/** A project that is only there for a while: the Scratch folder, a conflict-fix worktree Hub made
 *  (`<data>/fix/pr-<n>`) and the widgets' folder. The lists keep these apart from the projects you
 *  keep, under "Temporary". */
export function isTemporary(path: string): boolean {
  return isScratch(path) || isChats(path) || /[\\/]fix[\\/]pr-\d+(?:[\\/]|$)/.test(path) || /[\\/]widgets[\\/]?$/.test(path);
}

/** The project a folder belongs to: an attempt's worktree (`.git/hub-attempts/…`) or a dev-mode
 *  thread's (`.hub-threads/…`) counts as its repository, and a scratch thread's folder counts as
 *  the Scratch project. */
export function projectFolder(path: string): string {
  const scratch = SCRATCH_RUN.exec(path);
  if (scratch) return scratch[1];
  const chat = CHAT_RUN.exec(path);
  if (chat) return chat[1];
  // Overview widgets each have a folder under the data folder's widgets/ (widgets.rs): one project.
  const widget = /^(.*[\\/]widgets)[\\/]w[0-9a-f]{10}[\\/]?$/.exec(path);
  if (widget) return widget[1];
  const m = /^(.*?)[\\/](?:\.git[\\/]hub-attempts|\.hub-threads)[\\/][^\\/]+(.*)$/.exec(path);
  return m ? (m[1] + m[2]).replace(/[\\/]+$/, "") : path;
}

/** The project folder of a thread: the core's own answer (`Thread.project`, worked out once on the
 *  server) or, from a hubd that does not send it, the same worked out here from `cwd`. */
export function threadProject(t: { cwd: string; project?: string }): string {
  return t.project ?? projectFolder(t.cwd);
}

/** A version or commit id as shown in the UI: "dev" stays, a hash is cut to 7 characters, empty
 *  becomes `empty`. The one place that decides how a version reads. */
export function shortVersion(v: string | null | undefined, empty = "?"): string {
  if (!v) return empty;
  return /^[0-9a-f]{8,}$/.test(v) ? v.slice(0, 7) : v;
}

/** Text cut to `max` characters with an ellipsis; the cut end is trimmed. */
export function clip(text: string, max: number): string {
  return text.length > max ? text.slice(0, max - 1).trimEnd() + "…" : text;
}

export function ago(ms: number, now = Date.now()): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return "now";
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h`;
  return `${Math.round(h / 24)} d`;
}

/** `ago` as a phrase: "3 d ago", or "just now". */
export function since(ms: number): string {
  const a = ago(ms);
  return a === "now" ? "just now" : `${a} ago`;
}

/** When something happened, for the thread list: minutes within the hour, then the time of day,
 *  "Fri 22:08" within the week and a date after that. `relative` keeps the old "10 h". */
export function listTime(ms: number, now = Date.now(), style: "clock" | "relative" = "clock"): string {
  if (style === "relative") return ago(ms, now);
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return "now";
  if (s < 3600) return `${Math.round(s / 60)} min`;
  const then = new Date(ms);
  const day = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((day(new Date(now)) - day(then)) / 86_400_000);
  // Always 24-hour, "22:08", which is what fits the narrow column.
  const time = then.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
  if (days <= 0) return time;
  if (days < 7) return `${then.toLocaleDateString([], { weekday: "short" })} ${time}`;
  const year = then.getFullYear() !== new Date(now).getFullYear();
  return then.toLocaleDateString([], { day: "numeric", month: "short", ...(year ? { year: "numeric" } : {}) });
}

/** The full date and time, for a tooltip. */
export function fullTime(ms: number): string {
  return new Date(ms).toLocaleString([], { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
}

export function clock(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function bytes(n: number): string {
  const gb = n / 2 ** 30;
  return gb >= 1 ? `${gb.toFixed(1)} GB` : `${Math.round(n / 2 ** 20)} MB`;
}

/** Dollars: "$0", "<$0.0001", four decimals under a cent, else two. */
export function dollars(usd: number): string {
  return usd === 0 ? "$0" : usd < 0.0001 ? "<$0.0001" : `$${usd < 0.01 ? usd.toFixed(4) : usd.toFixed(2)}`;
}

/** Projects: added folders plus the folders threads run in, busiest first. */
/** Threads grouped into projects: one per folder and machine. `host` is null for this machine. */
export function byProject(
  threads: Thread[],
  added: Project[] = [],
): { key: string; name: string; cwd: string; host: string | null; threads: Thread[]; added: boolean; at: number }[] {
  const key = (cwd: string, host?: string | null) => (host ? `${host}\u0000${cwd}` : cwd);
  const groups = new Map<string, { cwd: string; host: string | null; threads: Thread[] }>();
  // The Chat world's folder is not a project: no list of projects holds it, whatever put it there.
  for (const p of added) if (!isChats(projectFolder(p.path))) groups.set(p.path, { cwd: p.path, host: null, threads: [] });
  for (const t of threads) {
    const cwd = projectFolder(t.cwd);
    if (isChats(cwd)) continue;
    const k = key(cwd, t.host);
    const group = groups.get(k) ?? { cwd, host: t.host ?? null, threads: [] };
    group.threads.push(t);
    groups.set(k, group);
  }
  const stored = new Map(added.map((p) => [p.path, p]));
  return [...groups.entries()]
    .map(([key, { cwd, host, threads }]) => {
      const p = host ? undefined : stored.get(cwd);
      return { key, name: p?.name ?? folderName(cwd), cwd, host, threads, added: !!p, at: threads[0]?.updated_at ?? p?.created_at ?? 0 };
    })
    .sort((a, b) => b.at - a.at);
}

export const statusLabel: Record<Thread["status"], string> = {
  idle: "Idle",
  running: "Running",
  waiting_on_you: "Waiting on you",
  error: "Failed",
  in_terminal: "In a terminal",
};

// "Oct 30, 1:50 AM GMT+1" reads as "Fri 1:50 AM" when it is within a week, as in Claude Code.
export function resets(r: string | null) {
  const text = r?.replace(/\s+(GMT|UTC)[+-]?[\d:]*$/, "") ?? "";
  const m = /^(\w{3}) (\d{1,2}), (.+)$/.exec(text);
  if (!m) return text;
  const now = new Date();
  let when = new Date(`${m[1]} ${m[2]}, ${now.getFullYear()} ${m[3]}`);
  if (when.getTime() < now.getTime() - 86_400_000) when = new Date(`${m[1]} ${m[2]}, ${now.getFullYear() + 1} ${m[3]}`);
  if (Number.isNaN(when.getTime()) || when.getTime() - now.getTime() > 6 * 86_400_000) return text;
  return weekdayTime(when);
}

function weekdayTime(when: Date) {
  const day = when.toLocaleDateString("en-US", { weekday: "short" });
  return `${day} ${when.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" })}`;
}

/** The moment a limit resets, as a time: the one the engine gave, else its words ("Nov 7, 4:00 PM
 *  GMT+1") read in the zone they name, or in this computer's when they name none. */
export function resetMoment(l: { resets: string | null; resets_at: number | null }): number | null {
  if (l.resets_at) return l.resets_at;
  const m = /^(\w{3}) (\d{1,2}), (\d{1,2}):(\d{2}) ?(AM|PM)(?:\s+(?:GMT|UTC)([+-]\d{1,2})(?::?(\d{2}))?)?$/i.exec(l.resets?.trim() ?? "");
  if (!m) return null;
  const month = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"].indexOf(m[1].toLowerCase());
  if (month < 0) return null;
  const now = new Date();
  const hour = (Number(m[3]) % 12) + (m[5].toUpperCase() === "PM" ? 12 : 0);
  const at = (year: number) => {
    if (m[6] === undefined) return new Date(year, month, Number(m[2]), hour, Number(m[4])).getTime();
    const east = Math.sign(Number(m[6]) || 1) * (Math.abs(Number(m[6])) * 60 + Number(m[7] ?? 0));
    return Date.UTC(year, month, Number(m[2]), hour, Number(m[4])) - east * 60_000;
  };
  let when = at(now.getFullYear());
  if (when < now.getTime() - 86_400_000) when = at(now.getFullYear() + 1);
  return Number.isNaN(when) ? null : when;
}

/** How long a limit's window is, from its name, so a bar can show how far into it you are. */
export function windowMs(label: string): number | null {
  if (/5[- ]?hour/i.test(label)) return 5 * 3_600_000;
  if (/weekly/i.test(label)) return 7 * 86_400_000;
  return null;
}

/** Days, hours, minutes and seconds in a span, the seconds rounded up so the clock ends on 0 and never skips it. */
export function countdown(ms: number): { d: number; h: number; m: number; s: number } {
  const secs = Math.max(0, Math.ceil(ms / 1000));
  return { d: Math.floor(secs / 86_400), h: Math.floor((secs % 86_400) / 3600), m: Math.floor((secs % 3600) / 60), s: secs % 60 };
}

/** When a plan limit resets, from the time the engine gave or else its own words. */
export function resetOf(l: { resets: string | null; resets_at: number | null }): string | null {
  if (l.resets_at) {
    const when = new Date(l.resets_at);
    if (when.getTime() - Date.now() <= 6 * 86_400_000) return weekdayTime(when);
    return when.toLocaleDateString("en-US", { month: "short", day: "numeric" });
  }
  return l.resets ? resets(l.resets) : null;
}

