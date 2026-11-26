// Spots commands that destroy work, so a permission question about one stands out and names
// what it would hit. A hint for the user, not a sandbox: the engine's own rules still apply.

export type Risk = {
  /** Short, plain reason, e.g. "Deletes files". */
  label: string;
  /** Paths the command names, to count what they hold. */
  targets: string[];
};

type Rule = { test: RegExp; label: string; targets?: (args: string[]) => string[] };

const paths = (args: string[]) => args.filter((a) => !a.startsWith("-") && !/^\/[a-z]$/i.test(a));

const RULES: Rule[] = [
  { test: /^(sudo\s+)?rm\s/, label: "Deletes files", targets: (a) => paths(a.slice(1)) },
  { test: /^rmdir\s/, label: "Deletes folders", targets: (a) => paths(a.slice(1)) },
  { test: /^(del|erase|rd|rmdir)\s.*\/s/i, label: "Deletes files", targets: (a) => paths(a.slice(1)) },
  {
    test: /^remove-item\s/i,
    label: "Deletes files",
    targets: (a) => paths(a.slice(1)).filter((x) => !/^\$/.test(x)),
  },
  { test: /^find\s.*\s-delete\b/, label: "Deletes every file it finds" },
  { test: /^git\s+reset\s+.*--hard/, label: "Throws away uncommitted changes" },
  { test: /^git\s+clean\s+.*-\w*f/, label: "Deletes untracked files" },
  { test: /^git\s+(checkout|restore)\s+(--\s+)?\.(\s|$)/, label: "Throws away uncommitted changes" },
  { test: /^git\s+push\s+.*(--force\b|-f\b|--force-with-lease)/, label: "Rewrites history on the remote" },
  { test: /^git\s+branch\s+.*-D\b/, label: "Deletes a branch that may not be merged" },
  { test: /^git\s+stash\s+(drop|clear)/, label: "Deletes stashed work" },
  { test: /^rsync\s.*--delete/, label: "Deletes files missing from the source" },
  { test: /^(mkfs|format|diskpart|dd\s.*of=)/i, label: "Erases a disk" },
  { test: /\b(drop\s+(table|database|schema)|truncate\s+table)\b/i, label: "Deletes data from a database" },
];

/** The risk of a shell command line, checking each command in a chain. */
export function commandRisk(line: string): Risk | null {
  // Targets after a `cd` are relative to where it went.
  let dir = "";
  for (const part of line.split(/&&|\|\||;|\|/)) {
    const cmd = part.trim();
    const cd = /^cd\s+(?:\/d\s+)?("[^"]+"|'[^']+'|\S+)$/i.exec(cmd);
    if (cd) {
      const to = cd[1].replace(/^["']|["']$/g, "");
      dir = /^([a-z]:|[\\/~])/i.test(to) ? to : dir ? `${dir}/${to}` : to;
      continue;
    }
    const rule = RULES.find((r) => r.test.test(cmd));
    if (!rule) continue;
    const targets = (rule.targets?.(words(cmd)) ?? []).map((t) =>
      dir && !/^([a-z]:|[\\/~])/i.test(t) ? `${dir}/${t}` : t,
    );
    return { label: rule.label, targets };
  }
  return null;
}

/** The risk of a tool call as ACP describes it: a delete, or a command line. */
export function toolRisk(call: Record<string, any> | undefined): Risk | null {
  if (!call) return null;
  const input = call.rawInput ?? {};
  const line = typeof input.command === "string" ? input.command : stripTicks(call.title);
  const risk = line ? commandRisk(line) : null;
  if (risk) return risk;
  if (call.kind === "delete") {
    const target = input.file_path ?? input.path;
    return { label: "Deletes files", targets: typeof target === "string" ? [target] : [] };
  }
  return null;
}

function stripTicks(title: unknown): string {
  if (typeof title !== "string") return "";
  const m = /`([^`]+)`/.exec(title);
  return m ? m[1] : "";
}

/** Splits a command line into words, keeping quoted parts together. */
function words(cmd: string): string[] {
  return [...cmd.matchAll(/"([^"]*)"|'([^']*)'|(\S+)/g)].map((m) => m[1] ?? m[2] ?? m[3]);
}
