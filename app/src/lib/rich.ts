// Gives an agent's reply Hub's visual vocabulary: a small grey glyph says what a thing is (project,
// thread, machine, pull request, branch, file, command, setting, local server) and colour only ever
// says state. Things Hub knows open on click and say more on hover. Works on sanitized HTML, so
// any engine's plain Markdown gets it; the writing guide in Hub's MCP instructions only makes the
// links surer.
import type { HostInfo } from "./bindings/HostInfo";
import type { Project } from "./bindings/Project";
import type { Thread } from "./bindings/Thread";
import type { ThreadPr } from "./bindings/ThreadPr";
import { ago, folderName } from "./format";
import { langOf, LANG_MARKS } from "./langs";
import { unread } from "./state.svelte";

export type Known = {
  threads: Thread[];
  projects: Project[];
  prs: Record<string, ThreadPr>;
  hosts: HostInfo[];
  branches: Record<string, string>;
};

const STATUS: Record<string, string> = {
  running: "working",
  waiting_on_you: "waiting on you",
  error: "failed",
  in_terminal: "in the terminal",
  idle: "idle",
};

// The same strokes as Icon.svelte, so glyphs in replies match the rest of Hub.
const GLYPHS = {
  folder: "M3 6a1 1 0 0 1 1-1h5l2 2h9a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z",
  file: "M6 3h8l4 4v14H6zM14 3v4h4",
  monitor: "M3 4h18v12H3zM8 20h8M12 16v4",
  pr: "M6 3a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM6 7v10M6 17a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM18 17a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM18 17V9a3 3 0 0 0-3-3h-4M13 4l-2 2 2 2",
  issue: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 11.5a.5.5 0 1 0 0 1 .5.5 0 0 0 0-1z",
  branch: "M6 3v12M18 9a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM6 21a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM18 9a9 9 0 0 1-9 9",
  globe: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM3 12h18M12 3c2.5 2.5 3.8 5.5 3.8 9s-1.3 6.5-3.8 9c-2.5-2.5-3.8-5.5-3.8-9S9.5 5.5 12 3z",
  plugin: "M9 4.5a2 2 0 0 1 4 0V6h4v4h-1.5a2 2 0 0 0 0 4H17v4h-4v-1.5a2 2 0 0 0-4 0V18H5v-4h1.5a2 2 0 0 0 0-4H5V6h4z",
  gear: "M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6zM19 12h2M3 12h2M12 3v2M12 19v2M17 7l1.4-1.4M5.6 18.4 7 17M17 17l1.4 1.4M5.6 5.6 7 7",
  key: "M14.5 9.5a4.5 4.5 0 1 0-4.2 4.5L9 15.3V18H6.3v2.7H3.5V18l6-6a4.5 4.5 0 0 0 5-2.5zM16 8h.01",
} as const;
type Glyph = keyof typeof GLYPHS;

const SETTINGS = ["account", "appearance", "notifications", "dictation", "chat", "engines", "safety", "projects", "extensions", "machines", "access", "updates", "whatsnew", "privacy", "about"];

const COMMIT = /^[0-9a-f]{7,40}$/;
const FILE = /^(?:[A-Za-z]:[\\/]|~?\/)?(?:[\w.@-]+[\\/])*[\w.@-]*\.[a-z][\w]{0,7}(?::\d+(?:-\d+)?)?$/i;
const FOLDER = /^(?:[A-Za-z]:)?(?:[\w.@ -]+[\\/])+$/;
const COMMAND = /^(?:npm|npx|pnpm|yarn|bun|node|cargo|rustup|git|gh|python3?|pip|uv|go|make|docker|kubectl|ssh|scp|curl|wget|cd|ls|cat|grep|rg|claude|codex|hubd|tauri|winget|choco|brew|apt|sudo|pwsh|powershell)\b(?: |$)/;
const SECRET = /^[A-Z][A-Z0-9_]*(?:TOKEN|KEY|SECRET|PASSWORD|PASS)[A-Z0-9_]*$/;
const PORT = /^(?:https?:\/\/)?(?:localhost|127\.0\.0\.1|0\.0\.0\.0)?:(\d{2,5})\/?$/;
const PULL_REPO = /github\.com\/([^/]+\/[^/]+)\/pull\/\d+/;
const PULL = /github\.com\/[^/]+\/[^/]+\/(pull|issues)\/(\d+)/;
/** A link to a GitHub repository itself (its front page, readme or a tree in it), not to a pull request or a file. */
const REPO = /^https?:\/\/github\.com\/([\w.-]+)\/([\w.-]+?)(?:\.git)?\/?(?:#readme)?(?:\/tree\/[^?#]*)?$/;
const NOT_OWNERS = new Set(["orgs", "settings", "marketplace", "topics", "features", "apps", "sponsors", "login", "about"]);
/** Extensions a file name in plain text must have to count as one. */
const CODE_EXT = "rs|ts|tsx|svelte|py|toml|md|css|html|go|sh|ya?ml|json";
const BRANCH = /^(?:claude|feat|feature|fix|chore|release|hotfix|dev)\/[\w./-]+$/;

export function enrich(html: string, known: Known): string {
  const doc = new DOMParser().parseFromString(`<div>${html}</div>`, "text/html");
  const root = doc.body.firstElementChild as HTMLElement;
  const threads = new Map(known.threads.map((t) => [t.id, t]));
  const prs = new Map(Object.entries(known.prs).map(([id, pr]) => [pr.number, { pr, thread: threads.get(id) }]));
  const branches = new Set(Object.values(known.branches));

  const glyph = (name: Glyph, tone?: string) => {
    const svg = doc.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("class", tone ? `g ${tone}` : "g");
    const path = doc.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("d", GLYPHS[name]);
    svg.append(path);
    return svg;
  };
  const mark = (kind: string, label: string, title: string, open?: Record<string, string>) => {
    const el = doc.createElement("span");
    el.className = `ref ${kind}`;
    el.dataset.tip = title;
    el.append(Object.assign(doc.createElement("span"), { className: "ref-label", textContent: label }));
    for (const [k, v] of Object.entries(open ?? {})) el.dataset[k] = v;
    if (open) {
      el.setAttribute("role", "link");
      el.tabIndex = 0;
    }
    return el;
  };
  const withGlyph = (el: HTMLElement, name: Glyph, tone?: string) => (el.prepend(glyph(name, tone)), el);

  const thread = (t: Thread, label = t.title) => {
    const el = mark(`thread ${t.status}`, label, `${t.title}\n${STATUS[t.status] ?? t.status} · ${folderName(t.cwd)}${t.host ? ` · ${t.host}` : ""} · ${ago(t.updated_at)} ago`, { open: "thread", id: t.id });
    el.prepend(Object.assign(doc.createElement("i"), { className: `dot ${t.status}${unread(t) ? " unread" : ""}` }));
    return el;
  };
  const project = (path: string, label: string) => withGlyph(mark("project", label, `Project · ${path}`, { open: "project", path }), "folder");
  const host = (h: HostInfo, label = h.name) => {
    const el = withGlyph(mark("host", label, `${h.local ? "This machine" : h.address || "Machine"} · ${h.state}`, { open: "machines" }), "monitor");
    if (h.state === "online") el.append(Object.assign(doc.createElement("i"), { className: "online" }));
    return el;
  };
  /** A pull request. `repo` is what the chat named it with: `owner/name`, a project's name, or nothing (the thread's own). */
  const pull = (n: number, label: string, href?: string, kind = "pull", repo = "") => {
    const hit = kind === "pull" ? prs.get(n) : undefined;
    const state = hit?.pr.state ?? "";
    const full = repo.includes("/") ? repo : "";
    const url = href ?? (full ? `https://github.com/${full}/pull/${n}` : undefined);
    const open: Record<string, string> | undefined = hit?.thread && !full ? { open: "pr", id: hit.thread.id, n: String(n) } : url ? { open: "url", href: url } : undefined;
    // A pull request gets the hover card (`PrCard.svelte`) in place of a tip; an issue keeps its tip.
    const el = mark(`pr ${state}`, label, kind === "pull" ? "" : `Issue #${n}`, open);
    if (kind === "pull") {
      delete el.dataset.tip;
      el.dataset.card = String(n);
      el.dataset.cardRepo = repo;
      if (!open) el.tabIndex = 0;
    }
    return withGlyph(el, kind === "pull" ? "pr" : "issue", state);
  };
  const port = (n: string, label: string) => withGlyph(mark("port mono", label, `Opens localhost:${n}`, { open: "url", href: `http://localhost:${n}` }), "globe");

  // Links: hub:// ones point at a thread, project or setting; GitHub ones at a pull request or
  // issue; local ones at a dev server. Other web links say which site they go to.
  for (const a of [...root.querySelectorAll("a[href]")]) {
    const href = a.getAttribute("href") ?? "";
    const label = a.textContent?.trim() || href;
    const hub = /^hub:\/\/(thread|project|settings)\/(.*)$/.exec(href);
    let el: HTMLElement | null = null;
    if (hub?.[1] === "thread") {
      const t = threads.get(decodeURIComponent(hub[2]));
      el = t ? thread(t, label) : mark("thread gone", label, "A thread Hub no longer has");
    } else if (hub?.[1] === "project") {
      el = project(decodeURIComponent(hub[2]), label);
    } else if (hub?.[1] === "settings") {
      const section = decodeURIComponent(hub[2]).split(/[/#]/)[0];
      el = withGlyph(mark("setting", label, "Opens Settings", { open: "settings", section: SETTINGS.includes(section) ? section : "" }), "gear");
    } else if (PULL.test(href)) {
      const [, kind, n] = PULL.exec(href)!;
      el = pull(Number(n), label, href, kind === "pull" ? "pull" : "issue", PULL_REPO.exec(href)?.[1] ?? "");
    } else if (REPO.test(href) && !NOT_OWNERS.has(REPO.exec(href)![1].toLowerCase())) {
      const [, owner, name] = REPO.exec(href)!;
      const full = `${owner}/${name}`;
      el = withGlyph(mark("plugin", label === href ? full : label, `${full} on GitHub\nOpens it in Plugins`, { open: "plugin", repo: full }), "plugin");
    } else if (PORT.test(href)) {
      el = port(PORT.exec(href)![1], label === href ? `:${PORT.exec(href)![1]}` : label);
    } else if (linkedFile(href)) {
      // A link to a file (what Codex writes: `[orders.csv](orders.csv)`, a `file:` address or a
      // Windows path) opens the file in Hub, as a path in code does.
      el = fileRef(linkedFile(href)!);
      if (label !== href) el.querySelector(".ref-label")!.textContent = label;
    } else if (/^https?:/.test(href)) {
      a.classList.add("ext");
      a.setAttribute("data-open", "url");
      a.setAttribute("data-href", href);
      a.setAttribute("data-tip", href);
      const site = new URL(href).hostname.replace(/^www\./, "");
      if (!label.includes(site)) a.after(Object.assign(doc.createElement("span"), { className: "site", textContent: site }));
    }
    if (el) a.replaceWith(el);
  }

  // Inline code: commits, commands, branches, files, folders, secrets and ports each get their look.
  for (const code of [...root.querySelectorAll("code")]) {
    if (code.closest("pre")) continue;
    const text = (code.textContent ?? "").trim();
    let el: HTMLElement | null = null;
    if (COMMIT.test(text) && /\d/.test(text) && /[a-f]/.test(text)) el = mark("commit mono", text.slice(0, 7), `Commit ${text}`);
    else if (COMMAND.test(text)) el = mark("cmd mono", text, "Click to copy", { open: "copy", text });
    else if (branches.has(text) || BRANCH.test(text)) el = withGlyph(mark("branch mono", text, `Branch ${text}`), "branch");
    else if (PORT.test(text)) el = port(PORT.exec(text)![1], text);
    else if (SECRET.test(text)) el = withGlyph(mark("secret", text, "A secret, by name only"), "key");
    else if (!/\s/.test(text) && FOLDER.test(text)) el = withGlyph(mark("path mono", text, text), "folder");
    else if (!/\s/.test(text) && FILE.test(text)) el = fileRef(text);
    if (el) code.replaceWith(el);
  }
  function linkedFile(href: string): string | null {
    if (!href || href.startsWith("#") || /^(?:https?|hub|mailto|tel|ftps?):/i.test(href)) return null;
    let path = href.replace(/[#?].*$/, "");
    try {
      path = decodeURIComponent(path);
    } catch {
      return null;
    }
    path = path.replace(/^file:\/*(?=[A-Za-z]:)/i, "").replace(/^file:\/\//i, "");
    if (/^[a-z][a-z0-9+.-]+:/i.test(path)) return null;
    return FILE.test(path) ? path : null;
  }
  function fileRef(text: string) {
    const [, name, line] = /^(.*?)(:\d+(?:-\d+)?)?$/.exec(text)!;
    const lang = langOf(name);
    const el = mark("path mono", name, lang ? `${lang.name} · ${text}` : text, { open: "file", path: name });
    // A file in a language Hub knows shows that language's logo instead of the plain file glyph.
    if (lang && LANG_MARKS) {
      const svg = doc.createElementNS("http://www.w3.org/2000/svg", "svg");
      svg.setAttribute("viewBox", "0 0 24 24");
      svg.setAttribute("class", "g lang");
      svg.setAttribute("aria-label", lang.name);
      const path = doc.createElementNS("http://www.w3.org/2000/svg", "path");
      path.setAttribute("d", lang.path);
      svg.append(path);
      el.prepend(svg);
    } else withGlyph(el, "file");
    if (line) el.append(Object.assign(doc.createElement("span"), { className: "line", textContent: line }));
    return el;
  }

  // Names in plain text: thread titles, machines, projects, pull requests and change counts.
  const titles = known.threads.filter((t) => t.title.length >= 8);
  const hosts = known.hosts.filter((h) => h.name.length >= 2);
  const folders = new Map<string, string>();
  for (const p of known.projects) folders.set(p.name, p.path);
  for (const t of known.threads) if (!t.host) folders.set(folderName(t.cwd), t.cwd);
  // Only names that cannot be an ordinary word ("Atlas", "hub-dev"), so "desk" stays a word.
  const projects = [...folders.keys()].filter((n) => n.length >= 4 && /[A-Z0-9_.-]/.test(n) && !hosts.some((h) => h.name === n));
  const byTitle = new Map(titles.map((t) => [t.title.toLowerCase(), t]));
  const byHost = new Map(hosts.map((h) => [h.name, h]));
  const alternatives = [
    `(?<title>${titles.map((t) => escape(t.title)).sort(longest).join("|") || "(?!)"})`,
    `(?<name>${[...hosts.map((h) => h.name), ...projects].map(escape).sort(longest).join("|") || "(?!)"})`,
    // `#695`, and with a prefix: `PR #695`, `hub#695`, `owner/hub#695`.
    `(?<pull>(?:PRs?\\s+|pull request\\s+)?(?:(?<prrepo>[\\w.-]+(?:\\/[\\w.-]+)?)?)#\\d{1,6})\\b`,
    `(?<diff>\\+\\d+ [−-]\\d+)\\b`,
    // A file name written as plain text, only with a code extension, so "Node.js" or "e.g." stay words.
    `(?<file>(?:[\\w.@-]+[\\\\/])*[\\w@-][\\w.@-]*\\.(?:${CODE_EXT})(?::\\d+(?:-\\d+)?)?)(?![\\w/\\\\])`,
  ];
  const find = new RegExp(`(?<![\\w#+])(?:${alternatives.join("|")})`, "gi");
  const walker = doc.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (n) => (n.parentElement?.closest("a, code, pre, .ref, .h") ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT),
  });
  const texts: Text[] = [];
  while (walker.nextNode()) texts.push(walker.currentNode as Text);
  for (const node of texts) {
    const text = node.data;
    const frag = doc.createDocumentFragment();
    let at = 0;
    for (const m of text.matchAll(find)) {
      const g = m.groups!;
      let el: Node | null = null;
      if (g.title) el = thread(byTitle.get(g.title.toLowerCase())!, g.title);
      else if (g.name && byHost.has(g.name)) el = host(byHost.get(g.name)!);
      else if (g.name && folders.has(g.name)) el = project(folders.get(g.name)!, g.name);
      else if (g.pull) {
        const n = Number(/#(\d+)$/.exec(g.pull)![1]);
        const repo = g.prrepo && !/^PRs?$/i.test(g.prrepo) ? g.prrepo : "";
        const word = /^(?:PRs?|pull request)\s/i.test(g.pull) || /^PRs?$/i.test(g.prrepo ?? "");
        // A name before the # counts only when it is a repository (`owner/name`) or a project Hub knows,
        // so `C#12` stays text; a bare `#12` counts when a thread's pull request has that number.
        const named = repo.includes("/") || [...folders.keys()].some((f) => f.toLowerCase() === repo.toLowerCase());
        if (repo ? named : word || prs.has(n)) el = pull(n, g.pull, undefined, "pull", named ? repo : "");
      }
      else if (g.file) el = fileRef(g.file);
      else if (g.diff) {
        const [add, del] = g.diff.split(" ");
        el = Object.assign(doc.createElement("span"), { className: "diff mono" });
        el.appendChild(Object.assign(doc.createElement("span"), { className: "add", textContent: add }));
        el.appendChild(doc.createTextNode(" "));
        el.appendChild(Object.assign(doc.createElement("span"), { className: "del", textContent: del.replace("-", "−") }));
      }
      if (!el) continue;
      frag.append(text.slice(at, m.index), el);
      at = m.index! + m[0].length;
    }
    if (at === 0) continue;
    frag.append(text.slice(at));
    node.replaceWith(frag);
  }

  blocks(doc, root);
  return root.innerHTML;
}

/** Headings with a hairline, notes and warnings as hairlined quotes, checklists, code block
 *  bars, and the reply's closing question marked once. */
function blocks(doc: Document, root: HTMLElement) {
  for (const el of [...root.children]) {
    const bold = el.tagName === "P" && el.childNodes.length === 1 && /^(STRONG|B)$/.test(el.firstElementChild?.tagName ?? "") && (el.textContent ?? "").length < 60;
    if (!/^H[1-6]$/.test(el.tagName) && !bold) continue;
    // The hairline sits beside one box of words, so a heading with code or a link in it still wraps as text.
    const text = Object.assign(doc.createElement("span"), { className: "h-text" });
    text.append(...el.childNodes);
    el.replaceChildren(text);
    el.classList.add("h");
  }

  for (const quote of [...root.querySelectorAll("blockquote")]) {
    const first = quote.querySelector("p");
    const alert = /^\s*\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]\s*/i.exec(first?.textContent ?? "");
    const lead = alert ? alert[1] : (first?.firstElementChild?.tagName === "STRONG" && first.firstChild === first.firstElementChild ? first.firstElementChild.textContent ?? "" : "");
    const tone = /warn|heads|important|careful|obs/i.test(lead) ? "warn" : /error|fail|caution|danger|fel/i.test(lead) ? "err" : lead ? "note" : "";
    if (!tone) continue;
    quote.classList.add("callout", tone);
    if (alert && first) {
      // GitHub's [!NOTE] marker becomes a word, like a bold lead would read.
      first.firstChild!.textContent = (first.firstChild!.textContent ?? "").replace(/^\s*\[![A-Z]+\]\s*/i, "");
      if (first.firstChild?.textContent === "" && first.childNodes[1]?.nodeName === "BR") first.childNodes[1].remove();
      const words: Record<string, string> = { NOTE: "Note", TIP: "Tip", IMPORTANT: "Important", WARNING: "Heads up", CAUTION: "Careful" };
      first.prepend(Object.assign(doc.createElement("strong"), { textContent: words[alert[1].toUpperCase()] }), " ");
    }
  }

  // A loose list puts the box inside the item's first paragraph.
  for (const box of [...root.querySelectorAll("li > input[type=checkbox], li > p:first-child > input[type=checkbox]")]) {
    const done = (box as HTMLInputElement).checked || box.hasAttribute("checked");
    box.closest("li")!.classList.add("task", done ? "done" : "open");
    box.replaceWith(Object.assign(doc.createElement("span"), { className: "tick", textContent: done ? "✓" : "○" }));
  }
  for (const list of root.querySelectorAll("ul, ol")) if (list.querySelector(":scope > li.task")) list.classList.add("tasks");

  for (const pre of [...root.querySelectorAll("pre")]) {
    const lang = /language-([\w+-]+)/.exec(pre.querySelector("code")?.className ?? "")?.[1] ?? "";
    const bar = Object.assign(doc.createElement("div"), { className: "bar" });
    bar.append(Object.assign(doc.createElement("span"), { textContent: lang }));
    const copy = Object.assign(doc.createElement("button"), { className: "copy", textContent: "Copy" });
    copy.dataset.open = "copy-block";
    const right = doc.createElement("span");
    right.className = "tools";
    // A block that is a command to run gets "Run in terminal" beside Copy; Markdown only shows it
    // on a thread's own page (`.canrun`), where there is a machine and a folder to run it in.
    if (runnable(lang, pre.querySelector("code")?.textContent ?? "")) {
      const run = doc.createElement("span");
      run.className = "run";
      const go = Object.assign(doc.createElement("button"), { className: "go", title: "Type it in a terminal on this thread's machine; you press Enter" });
      go.dataset.open = "run";
      go.innerHTML = `<svg viewBox="0 0 24 24" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M8 5v14l11-7z"/></svg>Run in terminal`;
      const more = Object.assign(doc.createElement("button"), { className: "more", title: "Another machine, or as administrator" });
      more.dataset.open = "run-more";
      more.innerHTML = `<svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 9l6 6 6-6"/></svg>`;
      run.append(go, more);
      right.append(run);
    }
    right.append(copy);
    bar.append(right);
    pre.prepend(bar);
  }

  const last = root.lastElementChild;
  if (last?.tagName === "P" && /\?\s*$/.test(last.textContent ?? "") && !last.classList.contains("h")) last.classList.add("ask");
}

const SHELLS = new Set(["sh", "bash", "shell", "zsh", "fish", "powershell", "pwsh", "ps1", "cmd", "bat", "console", "terminal"]);

/** Whether a code block is a shell command: a shell language, or no language and a first line that starts like one. */
export function runnable(lang: string, code: string) {
  const first = code.trim().split(/\r?\n/, 1)[0]?.replace(/^[$>]\s+/, "") ?? "";
  if (!first) return false;
  if (SHELLS.has(lang.toLowerCase())) return true;
  return !lang && (COMMAND.test(first) || /^[\w./~$-]+=\S*;/.test(first));
}

function escape(s: string) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function longest(a: string, b: string) {
  return b.length - a.length;
}
