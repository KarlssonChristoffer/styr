// The plain parts of the folder picker: its rows, fuzzy matching, path helpers and the scratch folders.
import type { GithubRepo } from "./bindings/GithubRepo";
import type { ProjectFolders } from "./bindings/ProjectFolders";
import { keyOf } from "./github.svelte";

/** The projects found on each machine, kept while Hub is open so the picker opens with them at once. */
export const found = new Map<string, ProjectFolders>();

export const lastSep = (s: string) => Math.max(s.lastIndexOf("/"), s.lastIndexOf("\\"));
export const norm = (p: string) => p.trim().replace(/[\\/]+$/, "").replace(/\\/g, "/").toLowerCase();
export const parentOf = (path: string) => {
  const p = path.replace(/[\\/]+$/, "");
  return p.slice(0, lastSep(p)) || p;
};
export const baseName = (path: string) => path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || path;
/**
 * How well `q` matches `text`, as in fzf: every letter in order, more for letters in a row and
 * at the start of a word. Null when it does not match.
 */
export function fuzzy(q: string, text: string): { score: number; hits: number[] } | null {
  const t = text.toLowerCase();
  const hits: number[] = [];
  let score = 0, from = 0;
  for (const ch of q.toLowerCase()) {
    if (ch === " ") continue;
    const i = t.indexOf(ch, from);
    if (i < 0) return null;
    const start = i === 0 || /[\\/\-_. ]/.test(t[i - 1]) || (text[i] !== t[i] && text[i - 1] === t[i - 1]);
    score += 1 + (hits.length && hits.at(-1) === i - 1 ? 4 : 0) + (start ? 5 : 0) - Math.min(3, i - from) * 0.3;
    hits.push(i);
    from = i + 1;
  }
  return { score: score - t.length * 0.02, hits };
}
/** A folder to start a thread in, on one machine. */
export type Entry = {
  path: string;
  name: string;
  where: string;
  host: string | null;
  machine: string;
  repo: boolean;
  branch: string | null;
  /** The branch origin calls its own, when known. */
  main: string | null;
  behind: number | null;
  key: string | null;
  touched: number;
  worktrees: number;
  worktreeOf: string | null;
  /** The last time a thread there was sent work, for Recent. */
  at?: number;
};
export type Row =
  | { kind: "project"; e: Entry; hits?: number[]; section?: string; also?: Entry[] }
  | { kind: "github"; r: GithubRepo; local: Entry[]; hits?: number[] }
  | { kind: "dir"; path: string; name: string; repo: boolean; hits?: number[] }
  | { kind: "make"; name: string; parent: string }
  | { kind: "clone"; url: string; target: string }
  | { kind: "path"; path: string };
/** On a branch other than the one origin calls its own. */
export const away = (e: Entry) => !!e.branch && (e.main ? e.branch !== e.main : !["main", "master"].includes(e.branch));
/** The name with the letters that matched in bold. */
export function parts(name: string, hits: number[] = []) {
  const set = new Set(hits);
  const out: { text: string; hit: boolean }[] = [];
  for (let i = 0; i < name.length; i++) {
    const hit = set.has(i);
    if (out.length && out.at(-1)!.hit === hit) out.at(-1)!.text += name[i];
    else out.push({ text: name[i], hit });
  }
  return out;
}

/**
 * A fresh, empty folder for a thread that is not about any project: `scratch/<date-time>` in the
 * projects folder of that machine.
 */
export async function scratchFolder(host: string | null, api: typeof import("./api").api, under = "scratch"): Promise<string> {
  const p = found.get(host ?? "") ?? (await api.projectFolders(host));
  const sep = p.sep || "/";
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  const stamp = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}${pad(d.getSeconds())}`;
  return api.makeFolder(`${p.root.replace(/[\\/]+$/, "")}${sep}${under}${sep}${stamp}`, false, host);
}

/** A folder for one everyday chat: `<projects root>/chats/<date-time>`, as the phone makes them. */
export const chatFolder = (host: string | null, api: typeof import("./api").api) => scratchFolder(host, api, "chats");

/** What the search matches on every machine: projects by name and folder, the same repository on several machines as one row, then GitHub repositories not cloned anywhere. */
export function searchRows(filter: string, entries: Entry[], ghRepos: GithubRepo[], clones: Map<string, Entry[]>, target: string | null, root: string | null): Row[] {
    const now = Date.now();
    const seen = new Set<string>();
    const scored = entries
      .filter((e) => (seen.has(`${(e.host ?? "")}|${norm(e.path)}`) ? false : (seen.add(`${(e.host ?? "")}|${norm(e.path)}`), true)))
      .map((e) => {
        const name = fuzzy(filter, e.name);
        // The folder it is in counts too ("games" finds games/spacegame); the remote only once a
        // slash is typed, or every long path would match three letters.
        const other = filter.includes("/") || filter.includes("\\")
          ? (fuzzy(filter, e.key ?? "") ?? fuzzy(filter, e.where))
          : fuzzy(filter, `${baseName(parentOf(e.path))}/${e.name}`);
        const score = Math.max(name ? name.score * 2 : -1, other ? other.score : -1);
        if (score < 0) return null;
        // A little for being recent, so of two equal matches the one in use wins.
        const fresh = e.at ? 2 : e.touched ? Math.max(0, 1.5 - (now - e.touched) / 6048e5) : 0;
        // Folders on the machine the thread is on come before the same names elsewhere.
        const near = e.host === target ? 0.5 : 0;
        return { kind: "project" as const, e, hits: name?.hits, score: score + fresh + near };
      })
      .filter((r): r is NonNullable<typeof r> => !!r)
      .sort((a, b) => b.score - a.score);
    // The same repository cloned on several machines is one row, opened where the thread is.
    const grouped: Row[] = [];
    const first = new Map<string, Extract<Row, { kind: "project" }>>();
    for (const r of scored) {
      const k = r.e.key?.toLowerCase();
      const head = k ? first.get(k) : undefined;
      if (head) (head.also ??= []).push(r.e);
      else {
        if (k) first.set(k, r);
        grouped.push(r);
      }
    }
    for (const r of grouped) {
      if (r.kind === "project" && r.also && r.e.host !== target) {
        const mine = r.also.find((a) => a.host === target);
        if (mine) {
          r.also = [r.e, ...r.also.filter((a) => a !== mine)];
          r.e = mine;
        }
      }
    }
    // Repositories on GitHub that are not cloned anywhere come after the clones.
    const onGithub = ghRepos
      .filter((r) => !r.archived && !clones.has(keyOf(r)))
      .map((r) => {
        const m = fuzzy(filter, r.name);
        const full = fuzzy(filter, r.full);
        const score = Math.max(m ? m.score * 2 : -1, full ? full.score : -1);
        return score < 0 ? null : { kind: "github" as const, r, local: [] as Entry[], hits: m?.hits, score };
      })
      .filter((r): r is NonNullable<typeof r> => !!r)
      .sort((a, b) => b.score - a.score)
      .slice(0, 15);
    const out: Row[] = [...grouped.slice(0, 50), ...onGithub];
    const exact = scored.some((r) => r.e.name.toLowerCase() === filter.toLowerCase() && r.e.host === target);
    if (root && !exact && !/[<>:"|?*]/.test(filter)) out.push({ kind: "make", name: filter, parent: root });
    return out;
}
