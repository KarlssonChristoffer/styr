// When the chat meets a file an agent made that Hub has no viewer for (an Excel sheet, a Blender
// file; `missingViewer` in `filePreview.ts`), it tells Problems once per kind and day: "No viewer
// for .xlsx files". The core stamps it as a missing viewer and Guard hands it to a fix thread that
// builds the viewer. Only the extension, the file's name and the thread go along, never its content.
import { api } from "./api";
import { missingViewer } from "./filePreview";
import { quietly } from "./quiet";

const KEY = "hub.noViewer";
const DAY = 24 * 3600 * 1000;
type Told = { ts: number; problem?: number };

function remembered(): Record<string, Told> {
  try {
    const all = JSON.parse(localStorage.getItem(KEY) ?? "{}") as Record<string, number | Told>;
    return Object.fromEntries(Object.entries(all).map(([k, v]) => [k, typeof v === "number" ? { ts: v } : v]));
  } catch {
    return {};
  }
}

const told = new Map<string, Told>(Object.entries(remembered()));

/** The problem each kind (by extension) is noted as, for the line in the chat that says so. */
export const gapProblems = $state<Record<string, number>>(Object.fromEntries(Object.entries(remembered()).flatMap(([k, v]) => (v.problem ? [[k, v.problem]] : []))));

function keep(ext: string, entry: Told) {
  told.set(ext, entry);
  try {
    localStorage.setItem(KEY, JSON.stringify({ ...remembered(), [ext]: entry }));
  } catch {
    // Without storage it is told again after a restart, which only counts one more hit.
  }
}

/** Notes a gap if `path` is a kind with no viewer and it has not been told today. */
export function noteMissingViewer(path: string, thread?: string | null) {
  const gap = missingViewer(path);
  if (!gap) return;
  const now = Date.now();
  if ((told.get(gap.ext)?.ts ?? 0) > now - DAY) return;
  keep(gap.ext, { ts: now, problem: told.get(gap.ext)?.problem });
  api
    .reportProblem({
      project: "Hub",
      source: "app",
      level: "warning",
      message: `No viewer for .${gap.ext} files`,
      kind: "NoViewer",
      trace: null,
      frames: [],
      crumbs: [],
      release: null,
      environment: null,
      extra: { ext: gap.ext, what: gap.what, file: path.split(/[\\/]/).pop() ?? path, thread: thread ?? null },
      fingerprint: ["no-viewer", gap.ext],
      ts: now,
    })
    .then((p) => {
      gapProblems[gap.ext] = p.id;
      keep(gap.ext, { ts: now, problem: p.id });
    })
    // Reporting must never make an error of its own.
    .catch(quietly("gap report"));
}
