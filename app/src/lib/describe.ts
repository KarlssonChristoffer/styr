// Says in plain words what a shell command line does, for a permission question: "Searches the
// code twice, shows git diff and runs tests". A reading aid, not a judge: risk.ts decides what
// is dangerous, and the whole command is always one click away.

type Kind = { key: string; one: string; many?: (n: number) => string };

const READ: Kind = { key: "read", one: "reads a file", many: (n) => `reads ${n} files` };
const SEARCH: Kind = { key: "search", one: "searches the code", many: (n) => `searches the code ${n === 2 ? "twice" : `${n} times`}` };
const LIST: Kind = { key: "list", one: "lists files" };
const TEST: Kind = { key: "test", one: "runs tests" };
const CHECK: Kind = { key: "check", one: "runs checks" };
const BUILD: Kind = { key: "build", one: "builds" };
const INSTALL: Kind = { key: "install", one: "installs packages" };
const WEB: Kind = { key: "web", one: "fetches from the web" };
const DELETE: Kind = { key: "delete", one: "deletes files" };
const MOVE: Kind = { key: "move", one: "moves or copies files" };
const MAKE: Kind = { key: "mkdir", one: "makes folders" };
const WRITE: Kind = { key: "write", one: "writes a file" };

const GIT: Record<string, string> = {
  log: "shows git log", show: "shows a commit", diff: "shows git diff", status: "shows git status", blame: "shows git blame",
  branch: "lists branches", fetch: "fetches from the remote", pull: "pulls from the remote", push: "pushes to the remote",
  commit: "commits", add: "stages files", checkout: "switches branch", switch: "switches branch", merge: "merges",
  rebase: "rebases", reset: "resets", stash: "stashes changes", restore: "restores files", clean: "removes untracked files",
  tag: "tags", "rev-parse": "reads git", "ls-files": "lists files in git", remote: "shows remotes", worktree: "manages worktrees",
};

/** What one command does, or null when Hub does not know it. */
function kindOf(cmd: string): Kind | string | null {
  const w = cmd.split(/\s+/);
  const head = w[0].toLowerCase().replace(/^sudo$/, w[1]?.toLowerCase() ?? "");
  if (/(^|\s)(>|>>)\s*(?!\/dev\/null|nul\b|&)\S/.test(cmd) || /^(set-content|out-file|tee)$/.test(head)) return WRITE;
  if (head === "git") {
    const sub = w.slice(1).find((x, i, a) => !x.startsWith("-") && a[i - 1] !== "-C" && a[i - 1] !== "-c") ?? "";
    return GIT[sub] ?? `runs git ${sub}`.trim();
  }
  if (head === "gh") return w[1] === "pr" ? `${w[2] === "create" ? "opens" : w[2] === "merge" ? "merges" : "looks at"} a pull request` : "talks to GitHub";
  if (/^(cat|head|tail|less|more|type|get-content|gc|nl|bat)$/.test(head) || (head === "sed" && w[1] === "-n")) return READ;
  if (/^(grep|egrep|rg|ag|ack|findstr|select-string|sls)$/.test(head)) return SEARCH;
  if (/^(ls|dir|tree|find|fd|get-childitem|gci|stat|du|wc)$/.test(head)) return LIST;
  if (/^(rm|rmdir|del|erase|rd|remove-item)$/.test(head)) return DELETE;
  if (/^(mv|cp|move|copy|move-item|copy-item|rsync)$/.test(head)) return MOVE;
  if (/^(mkdir|md|new-item)$/.test(head)) return MAKE;
  if (/^(curl|wget|invoke-webrequest|iwr|invoke-restmethod|irm)$/.test(head)) return WEB;
  const rest = w.slice(1).join(" ");
  if (/^(npm|pnpm|yarn|bun)$/.test(head)) {
    if (/^(i|install|add|ci|update|upgrade)\b/.test(rest)) return INSTALL;
    if (/\btest\b/.test(rest)) return TEST;
    if (/\b(check|lint|typecheck)\b/.test(rest)) return CHECK;
    if (/\bbuild\b/.test(rest)) return BUILD;
    return `runs ${head} ${w[1] === "run" ? w[2] ?? "" : w[1] ?? ""}`.trim();
  }
  if (head === "cargo") {
    if (/^(test|nextest)\b/.test(rest)) return TEST;
    if (/^(check|clippy|fmt)\b/.test(rest)) return CHECK;
    if (/^build\b/.test(rest)) return BUILD;
    if (/^(add|install|update)\b/.test(rest)) return INSTALL;
    if (/^run\b/.test(rest)) return "runs the program";
  }
  if (/^(pytest|vitest|jest|playwright)$/.test(head) || (head === "node" && w[1] === "--test") || /-m\s+pytest/.test(rest)) return TEST;
  if (/^(tsc|eslint|svelte-check|ruff|mypy|prettier)$/.test(head) || /^npx$/.test(head)) return /test|vitest|playwright/.test(rest) ? TEST : CHECK;
  if (/^(pip|pip3)$/.test(head) && /^install\b/.test(rest)) return INSTALL;
  if (/^(python|python3|py|node|bash|sh|pwsh|powershell)$/.test(head)) {
    const script = w.slice(1).find((x) => !x.startsWith("-"));
    return script ? `runs ${script.split(/[\\/]/).pop()}` : `runs ${head}`;
  }
  return null;
}

/** Filters a pipe runs output through; they don't change what the line does. */
const FILTER = /^(head|tail|wc|sort|uniq|grep|rg|findstr|select-string|select-object|cut|tr|less|more|cat|jq|awk|sed|column|out-string|format-table|measure-object)\b/i;

export type Described = { text: string; dir: string | null; steps: number };

/** A plain sentence for `line`, or null when most of it is unknown. */
export function describeCommand(line: string): Described | null {
  const counts = new Map<string, { kind: Kind | string; n: number }>();
  let dir: string | null = null;
  let steps = 0;
  let unknown = 0;
  // Split keeping what joined each part, so a part after `|` can be told apart.
  const parts = line.split(/(&&|\|\||;|\||\n)/);
  let joint = "";
  for (const raw of parts) {
    if (/^(&&|\|\||;|\||\n)$/.test(raw)) {
      joint = raw;
      continue;
    }
    const cmd = raw.trim().replace(/^\(|\)$/g, "");
    if (!cmd) continue;
    const cd = /^(cd|pushd|set-location)\s+(?:\/d\s+)?("[^"]+"|'[^']+'|\S+)$/i.exec(cmd);
    if (cd) {
      steps++;
      dir = cd[2].replace(/^["']|["']$/g, "").split(/[\\/]/).filter(Boolean).slice(-2).join("/");
      continue;
    }
    if (joint === "|" && FILTER.test(cmd)) continue;
    steps++;
    const kind = kindOf(cmd);
    if (!kind) {
      unknown++;
      const word = cmd.split(/\s+/)[0];
      const key = `runs ${word}`;
      counts.set(key, { kind: key, n: 1 });
      continue;
    }
    const key = typeof kind === "string" ? kind : kind.key;
    const had = counts.get(key);
    counts.set(key, { kind, n: (had?.n ?? 0) + 1 });
  }
  const said = [...counts.values()].map(({ kind, n }) => (typeof kind === "string" ? kind : n > 1 && kind.many ? kind.many(n) : kind.one));
  if (!said.length || unknown > said.length / 2) return null;
  const text = said.length === 1 ? said[0] : `${said.slice(0, -1).join(", ")} and ${said.at(-1)}`;
  return { text: text[0].toUpperCase() + text.slice(1), dir, steps };
}
