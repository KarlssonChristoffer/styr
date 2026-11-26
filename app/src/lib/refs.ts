// Hub's visual vocabulary, shared by every page and the chat: a grey glyph says what a thing is,
// and colour only ever says state, on one scale (see `.dot` in app.css for threads).
import { projectFolder } from "./format";
import { hub } from "./state.svelte";
import type { MachineKind } from "./bindings/MachineKind";
import { looks } from "./looks.svelte";

/** Glyph strokes on a 24 grid, in the style of Icon.svelte. */
export const GLYPHS = {
  project: "M3 6a1 1 0 0 1 1-1h5l2 2h9a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z",
  folder: "M3 6a1 1 0 0 1 1-1h5l2 2h9a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z",
  file: "M6 3h8l4 4v14H6zM14 3v4h4",
  doc: "M5 3h14v18H5zM9 8h6M9 12h6M9 16h3",
  repo: "M5 4.5A1.5 1.5 0 0 1 6.5 3H19v15H6.5A1.5 1.5 0 0 0 5 19.5zM5 19.5A1.5 1.5 0 0 0 6.5 21H19M9 7h6",
  monitor: "M3 4h18v12H3zM8 20h8M12 16v4",
  laptop: "M5 5h14v10H5zM2.5 18.5h19l-1.5-3.5H4z",
  board: "M5 5h14v14H5zM9.5 9.5h5v5h-5zM9 2.5V5M15 2.5V5M9 19v2.5M15 19v2.5M2.5 9H5M2.5 15H5M19 9h2.5M19 15h2.5",
  server: "M4 4h16v7H4zM4 13h16v7H4zM8 7.5h.01M8 16.5h.01",
  cloud: "M7 18a4.5 4.5 0 0 1-.6-9A6 6 0 0 1 18 9.5a4.3 4.3 0 0 1-.5 8.5z",
  // A thread that runs on a company's servers: a cloud with an arrow going up into it. Not `cloud`, which is a VPS.
  cloudrun: "M7 18a4.5 4.5 0 0 1-.6-9A6 6 0 0 1 18 9.5a4.3 4.3 0 0 1-.5 8.5zM12 15.5v-5M9.8 12.6l2.2-2.2 2.2 2.2",
  pr: "M6 3a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM6 7v10M6 17a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM18 17a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM18 17V9a3 3 0 0 0-3-3h-4M13 4l-2 2 2 2",
  issue: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 11.5a.5.5 0 1 0 0 1 .5.5 0 0 0 0-1z",
  branch: "M6 3v12M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM18 9a9 9 0 0 1-9 9",
  port: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM3 12h18M12 3c2.5 2.5 3.8 5.5 3.8 9s-1.3 6.5-3.8 9c-2.5-2.5-3.8-5.5-3.8-9S9.5 5.5 12 3z",
  process: "M4 5h16v14H4zM8 10l3 2-3 2M13 15h3",
  setting: "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM19 12h2M3 12h2M12 3v2M12 19v2M17 7l1.4-1.4M5.6 18.4 7 17M17 17l1.4 1.4M5.6 5.6 7 7",
  robot: "M5 8h14v11H5zM12 4.5V8M12 3.5h.01M9.5 12.5h.01M14.5 12.5h.01M9.5 16h5M3 12.5v3M21 12.5v3",
  clock: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 7v5l3 2",
  fork: "M6 3a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM18 3a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM12 17a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM6 7v3a3 3 0 0 0 3 3h6a3 3 0 0 0 3-3V7M12 13v4",
  shield: "M12 3l7 3v5.5c0 4.4-3 8-7 9.5-4-1.5-7-5.1-7-9.5V6z",
  secret: "M14.5 9.5a4.5 4.5 0 1 0-4.2 4.5L9 15.3V18H6.3v2.7H3.5V18l6-6a4.5 4.5 0 0 0 5-2.5zM16 8h.01",
} as const;
export type Glyph = keyof typeof GLYPHS;

const KIND_GLYPH: Record<MachineKind, Glyph> = { desktop: "monitor", laptop: "laptop", board: "board", vps: "cloud", server: "server" };

/** The glyph of a machine, by the kind of computer it is: the one picked for it, else what its
 *  hubd says it is (asked once, in the background), else a guess from its name. */
export function machineGlyph(name: string, local: boolean): Glyph {
  looks.learn(name, local);
  return KIND_GLYPH[looks.machineKind(name, local)];
}

/** A path as people read it: the home folder as ~, with the full path left for the hover. */
export function shortPath(path: string): string {
  const p = projectFolder(path);
  return p.replace(/^(?:\/home\/[^/]+|\/Users\/[^/]+|[A-Za-z]:\\Users\\[^\\]+|\/root)(?=[\\/]|$)/, "~");
}

/** `owner/name` of a GitHub remote URL, or null. */
export function repoName(remote: string | null | undefined): string | null {
  const m = /github\.com[:/]([^/\s]+\/[^/\s]+?)(?:\.git)?\/?$/.exec(remote ?? "");
  return m ? m[1] : null;
}

/** Opens the project a repository's folder belongs to, or the folder itself as one. */
export function openRepo(path: string) {
  const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/$/, "").toLowerCase();
  const at = norm(path);
  const p = hub.projects.find((p) => at === norm(p.path) || at.startsWith(norm(p.path) + "/"));
  hub.openProject(p?.path ?? path);
}
