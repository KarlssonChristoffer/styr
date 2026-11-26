<!-- DOMPurify's own list of safe link schemes, plus Hub's links to its threads and projects, and to files (a `file:` address or a Windows path, which agents write).
     Used by ChildLine.svelte, DayCard.svelte, DesignCard.svelte, DevlogCard.svelte and 11 more. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import DOMPurify from "dompurify";
  import { marked } from "marked";
  import { enrich } from "../rich";
  import { hub, type SettingsSection } from "../state.svelte";
  import { openUrl } from "../native";
  import { preview, previewable } from "../preview";
  import { mount, onDestroy, unmount } from "svelte";
  import { contextMenu, copy } from "../menu.svelte";
  import { hubdHas } from "../compat";
  import { commandText, isWindows, targetsFor, type RunTarget } from "../runInTerminal";
  import RunTerminal from "./RunTerminal.svelte";
  import { projectFolder } from "../format";
  import { figures } from "../figures";
  import { prefs } from "../prefs.svelte";
  import { reveal } from "../menus.svelte";
  import { folded, type ImageItem } from "../transcript";
  import { openPicture } from "../lightbox.svelte";
  import { dirHints, findFile, missingViewer, showable } from "../filePreview";
  import { api } from "../api";
  import { gapProblems, noteMissingViewer } from "../viewerGaps.svelte";
  import type { Thread } from "../bindings/Thread";
  import PrCard from "./PrCard.svelte";
  import type { CardKey } from "../prCard.svelte";

  /** `rich` gives agent replies Hub's visual vocabulary: glyphs for what things are, colour for state.
   *  `thread` is the thread the text is about, when it is not the one open (a hover card). */
  let { text, rich = false, thread }: { text: string; rich?: boolean; thread?: Thread } = $props();

  // DOMPurify's own list of safe link schemes, plus Hub's links to its threads and projects, and to files (a `file:` address or a Windows path, which agents write).
  const URIS = /^(?:(?:(?:f|ht)tps?|mailto|tel|hub|file):|[a-z]:[\\/%]|[^a-z]|[a-z+.\-]+(?:[^a-z+.\-:]|$))/i;
  const clean = $derived(DOMPurify.sanitize(marked.parse(text, { async: false, gfm: true, breaks: true }), { ALLOWED_URI_REGEXP: URIS }));
  const html = $derived(
    rich ? enrich(clean, { threads: hub.threads, projects: hub.projects, prs: hub.prs, hosts: hub.hosts, branches: hub.branches }) : clean,
  );

  // A file the reply names that Hub has no viewer for is noted in Problems (once per kind and day),
  // and one quiet line under the text says so.
  let gaps = $state<{ ext: string; what: string; file: string }[]>([]);
  $effect(() => {
    void html;
    if (!rich || thread || !box) {
      gaps = [];
      return;
    }
    const id = hub.current?.id;
    const found = new Map<string, { ext: string; what: string; file: string }>();
    for (const el of box.querySelectorAll<HTMLElement>('[data-open="file"][data-path]')) {
      const path = el.dataset.path!;
      const gap = missingViewer(path);
      if (!gap) continue;
      noteMissingViewer(path, id);
      if (!found.has(gap.ext)) found.set(gap.ext, { ...gap, file: path.split(/[\\/]/).pop() ?? path });
    }
    if (found.size !== gaps.length || [...found.keys()].some((k, i) => k !== gaps[i]?.ext)) gaps = [...found.values()];
  });
  function openGap(ext: string) {
    const problem = gapProblems[ext];
    if (!problem) return;
    hub.problemPick = { id: problem, host: null };
    hub.show({ view: "problems" });
  }

  // Formulas and Mermaid diagrams, drawn once the text has stopped changing for a moment.
  let box: HTMLDivElement;
  $effect(() => {
    void html;
    const math = prefs.math;
    if (!/class="tex"|language-(mermaid|math)/.test(html)) return;
    const timer = setTimeout(() => quiet(figures(box, math), "figures"), 300);
    return () => clearTimeout(timer);
  });

  // ---- Pull request cards: hover after a short rest on a pull request, a tap where there is no hover ----
  let prCard = $state<{ el: HTMLElement; key: CardKey; number: number; sheet: boolean } | null>(null);
  let prTimer = 0;
  const prThread = $derived(prCard ? hub.prs && Object.entries(hub.prs).find(([, p]) => p.number === prCard!.number) : undefined);
  const noHover = () => matchMedia("(hover: none)").matches;

  /** Where a reference's pull request lives: this thread's project, another project named in the text, or `owner/name`. */
  function cardFor(el: HTMLElement): { key: CardKey; number: number } | null {
    const t = thread ?? hub.current;
    const number = Number(el.dataset.card);
    if (!t || !number) return null;
    const named = el.dataset.cardRepo ?? "";
    const project = named && !named.includes("/") ? hub.projects.find((p) => p.name.toLowerCase() === named.toLowerCase()) : undefined;
    const host = t.host && t.host !== hub.localHost ? t.host : null;
    return { number, key: { path: project?.path ?? t.cwd, repo: named.includes("/") ? named : "", number, host } };
  }
  function prEnter(e: Event) {
    if (noHover() || !rich) return;
    const el = (e.target as HTMLElement).closest<HTMLElement>("[data-card]");
    if (!el || (prCard && prCard.el === el)) return clearTimeout(prTimer);
    const c = cardFor(el);
    if (!c) return;
    clearTimeout(prTimer);
    prTimer = window.setTimeout(() => el.isConnected && (prCard = { el, ...c, sheet: false }), prCard ? 80 : 400);
  }
  function prLeave(e: Event) {
    if (!(e.target as HTMLElement).closest?.("[data-card]")) return;
    clearTimeout(prTimer);
    prTimer = window.setTimeout(() => prCard && !prCard.sheet && (prCard = null), 260);
  }
  const prKeep = () => clearTimeout(prTimer);
  const prClose = () => {
    clearTimeout(prTimer);
    prCard = null;
  };
  onDestroy(() => clearTimeout(prTimer));
  function prOpen() {
    const t = prThread?.[0] && hub.threads.find((x) => x.id === prThread[0]);
    if (!t || !prCard) return;
    hub.openProject(t.cwd, t.host ?? null);
    hub.github = { mode: "pulls", pull: prCard.number, issue: null };
    hub.projectTab = "github";
  }

  function go(e: MouseEvent | KeyboardEvent) {
    if (e instanceof KeyboardEvent && e.key !== "Enter") return;
    const target = e.target as HTMLElement;
    // Where there is no hover, a tap on a pull request shows its card, which has the ways in.
    const card = rich && noHover() ? target.closest<HTMLElement>("[data-card]") : null;
    if (card) {
      const c = cardFor(card);
      if (c) {
        e.preventDefault();
        prCard = { el: card, ...c, sheet: true };
        return;
      }
    }
    const a = target.closest("a");
    if (a?.getAttribute("href")?.startsWith("hub:")) e.preventDefault();
    const el = target.closest<HTMLElement>("[data-open]");
    if (!el) return;
    e.preventDefault();
    const { open, id, path, n, href, section, text, repo } = el.dataset;
    if (open === "thread" && id) hub.open(id);
    else if (open === "plugin" && repo) hub.openPlugins(repo);
    else if (open === "project" && path) hub.openProject(projectFolder(path), hub.threads.find((t) => t.cwd === path)?.host ?? null);
    else if (open === "pr" && id) {
      const t = hub.threads.find((t) => t.id === id);
      if (!t) return;
      hub.openProject(t.cwd, t.host ?? null);
      hub.github = { mode: "pulls", pull: Number(n), issue: null };
      hub.projectTab = "github";
    } else if (open === "url" && href) {
      // A dev server opens beside the thread it was named in; Ctrl-click, or anywhere else, the browser.
      const local = hub.view === "thread" && hub.current && !(e instanceof MouseEvent && (e.ctrlKey || e.metaKey)) && previewable(href);
      if (local) preview(href);
      else openUrl(href).catch((err) => hub.fail(err));
    }
    else if (open === "settings") hub.openSettings((section || undefined) as SettingsSection | undefined);
    else if (open === "machines") hub.openSettings("machines");
    else if (open === "file" && path) openFile(el, path, e instanceof MouseEvent && (e.ctrlKey || e.metaKey));
    else if (open === "copy" && text) copied(el, text);
    else if (open === "copy-block") copied(el, el.closest("pre")?.querySelector("code")?.textContent ?? "");
    else if (open === "run" || open === "run-more") {
      const pre = el.closest("pre");
      if (pre) void (open === "run" ? run(pre) : runMenu(pre, el));
    }
  }

  // "Run in terminal" on a command: a terminal under the block with the command typed and not run
  // (RunTerminal.svelte). One per block; pressing again closes the old one and opens the new.
  const canrun = $derived(rich && !thread && hub.view === "thread" && !!hub.current);
  const runs = new Map<Element, { host: HTMLElement; app: Record<string, unknown> }>();
  onDestroy(() => {
    for (const r of runs.values()) void unmount(r.app);
    runs.clear();
  });

  /** Whether the administrator switch is offered on a machine, and how: a root shell, an elevated terminal inside Hub (Windows here), or a window outside Hub (Windows here, older Hub). */
  async function adminOn(name: string) {
    const windows = await isWindows(name);
    if (windows === null) return { canAdmin: false, outside: false, elevated: false };
    if (!windows) return { canAdmin: true, outside: false, elevated: false };
    // A Windows machine reached over SSH is already elevated for an administrator; here Windows asks (UAC).
    if (name !== hub.localHost) return { canAdmin: false, outside: false, elevated: false };
    const elevated = await quiet(hubdHas("machine.admin.terminal"), "hubd has", false); // compat-until: 2026-12-03
    const outside = !elevated && (await quiet(hubdHas("machine.admin"), "hubd has", false)); // compat-until: 2026-12-03
    return { canAdmin: elevated || outside, outside, elevated };
  }

  async function run(pre: Element, target?: RunTarget, admin = false) {
    const t = thread ?? hub.current;
    if (!t) return;
    const to = target ?? targetsFor(t)[0];
    const command = commandText(pre.querySelector("code")?.textContent ?? "");
    if (!command) return;
    const found = await adminOn(to.name);
    const old = runs.get(pre);
    if (old) {
      void unmount(old.app);
      old.host.remove();
    }
    const host = document.createElement("div");
    pre.after(host);
    const app = mount(RunTerminal, {
      target: host,
      props: {
        command,
        machine: to.name,
        cwd: to.cwd,
        admin: admin && found.canAdmin,
        canAdmin: found.canAdmin,
        outside: found.outside,
        elevated: found.elevated,
        onclose: () => {
          void unmount(app);
          host.remove();
          runs.delete(pre);
        },
      },
    });
    runs.set(pre, { host, app });
    // The card can open below the visible part of the chat.
    requestAnimationFrame(() => host.scrollIntoView({ block: "nearest", behavior: "smooth" }));
  }

  /** The arrow on the button: another machine, or as administrator. Never remembered. */
  async function runMenu(pre: Element, at: HTMLElement) {
    const t = thread ?? hub.current;
    if (!t) return;
    const targets = targetsFor(t);
    const admin = await Promise.all(targets.map((x) => adminOn(x.name)));
    const r = at.getBoundingClientRect();
    contextMenu.show(r.right, r.bottom + 4, [
      { label: "Run on", heading: true },
      ...targets.map((x) => ({ label: x.name, icon: "terminal" as const, hint: x.here ? "this thread" : undefined, run: () => run(pre, x) })),
      targets.some((_, i) => admin[i].canAdmin) ? null : undefined,
      ...targets.flatMap((x, i) =>
        admin[i].canAdmin ? [{ label: `${x.name} as administrator`, icon: "shield" as const, hint: admin[i].outside ? "a window outside Hub" : undefined, run: () => run(pre, x, true) }] : [],
      ),
    ]);
  }

  // A picture the thread or one of its side jobs made opens big over the window, found by where it was
  // saved or else by its file name. A file Hub can read (Markdown, JSON, CSV, PDF, code, …) opens
  // in the viewer as what it is; Ctrl-click, or anything else with a full path, shows its folder.
  function openFile(el: HTMLElement, path: string, inFolder = false) {
    const t = thread ?? hub.current;
    const key = (p: string) => p.replace(/\\/g, "/").toLowerCase();
    const base = (p: string) => key(p).split("/").pop();
    if (t) {
      const images = [t, ...hub.childrenOf(t.id)].flatMap((th) =>
        folded(hub.events[th.id] ?? []).filter((i): i is ImageItem => i.kind === "image" && !!i.path).map((image) => ({ thread: th.id, image })),
      );
      const hit = images.find((x) => key(x.image.path!) === key(path)) ?? images.find((x) => base(x.image.path!) === base(path));
      if (hit) {
        openPicture(hit.thread, hit.image.id);
        return;
      }
    }
    if (t && !inFolder && showable(path)) {
      // A bare name (`order.csv`) is where a script put it, which the reply may only hint at.
      const id = t.id;
      const bare = !/^(?:[A-Za-z]:[\\/]|\/)/.test(path);
      if (!bare) hub.viewing = { thread: id, kind: "file", id: path };
      else
        findFile((d) => api.projectTree(t.cwd, d, t.host ?? null), path, dirHints(el.closest(".md")?.textContent ?? ""))
          .then((rel) => (hub.viewing = { thread: id, kind: "file", id: rel }))
          .catch(() => (hub.viewing = { thread: id, kind: "file", id: path }));
      return;
    }
    if (/^(?:[A-Za-z]:[\\/]|\/)/.test(path) && !t?.host) reveal(path)();
    else copied(el, path);
  }

  function copied(el: HTMLElement, text: string) {
    copy(text)
      .then(() => {
        el.classList.add("copied");
        setTimeout(() => el.classList.remove("copied"), 1200);
      })
      .catch((err) => hub.fail(err));
  }
</script>

<div role="presentation" class="md" class:rich class:canrun bind:this={box} onclick={go} onkeydown={go} onmouseover={prEnter} onmouseout={prLeave} onfocusin={prEnter} onfocusout={prLeave}>{@html html}</div>
{#if prCard}
  {#key `${prCard.number}|${prCard.key.repo}|${prCard.sheet}`}
    <PrCard
      anchor={prCard.el}
      cardKey={prCard.key}
      known={hub.prs[prThread?.[0] ?? ""]}
      owner={hub.threads.find((x) => x.id === prThread?.[0])?.title}
      sheet={prCard.sheet}
      onkeep={prKeep}
      onleave={() => prLeave({ target: prCard!.el } as unknown as Event)}
      onclose={prClose}
      onopen={prThread ? prOpen : undefined}
    />
  {/key}
{/if}
{#each gaps as g (g.ext)}
  <div class="gap">
    <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 7l8-4 8 4v10l-8 4-8-4zM4 7l8 4 8-4M12 11v10" /></svg>
    <span><b>{g.file}</b> · {g.what} · no viewer yet{#if gapProblems[g.ext]} · noted as <button class="link" data-tip="Open it in Problems" onclick={() => openGap(g.ext)}>Problem #{gapProblems[g.ext]}</button>{/if}</span>
  </div>
{/each}

<style>
  /* One quiet grey line for a file Hub cannot show yet; colour is only for state. */
  .gap { display: flex; align-items: center; gap: 8px; color: var(--muted); font-size: 13px; margin: 6px 0 2px; }
  .gap svg { flex: none; }
  .gap b { font-weight: 400; color: var(--text-3); }
  .gap .link { background: none; border: 0; padding: 0; font: inherit; color: var(--muted); text-decoration: underline; text-underline-offset: 2px; cursor: pointer; }
  .gap .link:hover { color: var(--text-2); }
  .md { color: var(--text-2); overflow-wrap: anywhere; }
  .md :global(p) { margin: 0 0 0.7em; }
  .md :global(p:last-child) { margin-bottom: 0; }
  .md :global(code) { background: var(--surface-2); padding: 1px 4px; border-radius: var(--r-xs); }
  .md :global(pre) { background: var(--bg-sidebar); border: 1px solid var(--line); border-radius: var(--r-md); padding: 10px 12px; overflow-x: auto; }
  .md :global(pre code) { background: none; padding: 0; }
  .md :global(a) { color: var(--accent); }
  .md :global(ul), .md :global(ol) { padding-left: 1.3em; margin: 0 0 0.7em; }
  .md :global(h1), .md :global(h2), .md :global(h3) { color: var(--text); font-size: 1.05em; margin: 1em 0 0.4em; }
  /* Wide tables and pictures scroll or shrink inside the reply instead of pushing past it. */
  .md :global(table) { border-collapse: collapse; display: block; max-width: 100%; overflow-x: auto; overflow-wrap: normal; }
  .md :global(img) { max-width: 100%; height: auto; }
  .md :global(.math-block) { display: block; overflow-x: auto; margin: 0.4em 0 0.8em; }
  .md :global(.diagram) { overflow-x: auto; margin: 0.4em 0 0.8em; padding: 12px; background: var(--bg-sidebar); border: 1px solid var(--line); border-radius: var(--r-md); }
  .md :global(.diagram svg) { max-width: 100%; height: auto; }
  .md :global(td), .md :global(th) { border: 1px solid var(--line); padding: 3px 8px; }

  /* Agent replies: a grey glyph says what a thing is, colour only says state. */
  .rich :global(li::marker) { color: var(--muted); }
  /* A ref is text: a long one wraps between its words, and only its glyph stays with the first word. */
  .rich :global(.ref) { color: var(--text); white-space: nowrap; }
  .rich :global(.ref-label) { white-space: normal; }
  .rich :global(.ref[data-open]) { cursor: pointer; }
  .rich :global(.ref[data-open]:hover .ref-label), .rich :global(a.ext:hover) { text-decoration: underline; text-underline-offset: 3px; text-decoration-color: var(--line-strong); }
  .rich :global(.mono) { font-family: var(--mono); font-size: 0.88em; }
  .rich :global(.g) { width: 0.86em; height: 0.86em; margin-right: 4px; vertical-align: -0.08em; fill: none; stroke: var(--muted); stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
  .rich :global(.g.open), .rich :global(.g.draft) { stroke: var(--green); }
  .rich :global(.g.merged) { stroke: var(--accent); }
  .rich :global(.g.closed) { stroke: var(--red); }
  /* A language's logo (langs.ts) is filled, in the same grey as the stroked glyphs. */
  .rich :global(.g.lang) { fill: var(--text-2); stroke: none; width: 1.05em; height: 1.05em; vertical-align: -0.18em; }
  .rich :global(.thread .dot) { margin: 0 6px 1px 1px; vertical-align: middle; }
  .rich :global(.thread.gone) { color: var(--muted); text-decoration: line-through; }
  .rich :global(.online) { display: inline-block; width: 5px; height: 5px; margin-left: 4px; border-radius: 50%; background: var(--green); vertical-align: 0.15em; }
  .rich :global(.path .line) { color: var(--muted); }
  .rich :global(.commit) { color: var(--muted); }
  .rich :global(.cmd) { color: var(--text-3); background: var(--surface-2); padding: 1px 4px; border-radius: var(--r-xs); }
  .rich :global(.cmd)::before { content: "› "; color: var(--muted); }
  .rich :global(.copied)::after { content: " copied"; color: var(--green); font-family: var(--font, inherit); }
  .rich :global(a.ext) { color: var(--text); text-decoration: underline; text-decoration-color: var(--line-strong); text-underline-offset: 3px; cursor: pointer; }
  .rich :global(.site) { margin-left: 4px; color: var(--muted); font-size: 0.85em; }
  .rich :global(.diff .add) { color: var(--green); }
  .rich :global(.diff .del) { color: var(--red); }

  .rich :global(.h) { display: flex; align-items: center; gap: 10px; margin: 1em 0 0.3em; color: var(--text); font-size: 0.93em; font-weight: 600; }
  .rich :global(.h-text) { min-width: 0; }
  .rich :global(.h)::after { content: ""; flex: 1 0 24px; height: 1px; background: var(--line); }
  .rich :global(.h:first-child) { margin-top: 0; }
  .rich :global(blockquote) { margin: 0.2em 0 0.8em; padding: 1px 0 1px 12px; border-left: 2px solid var(--line-strong); color: var(--text-3); }
  .rich :global(.callout.warn) { border-color: var(--amber); }
  .rich :global(.callout.err) { border-color: var(--red); }
  .rich :global(.callout strong:first-child) { color: var(--text); font-size: 0.92em; margin-right: 2px; }
  .rich :global(.tasks) { list-style: none; padding-left: 0; }
  /* The tick hangs in the margin and the item stays ordinary text, so a bold lead, refs and the rest
     wrap as one paragraph. */
  .rich :global(.task) { position: relative; padding-left: 1.5em; }
  .rich :global(.task .tick) { position: absolute; left: 0; width: 1em; text-align: center; color: var(--muted); }
  .rich :global(.task.done .tick) { color: var(--green); }
  .rich :global(pre) { position: relative; padding: 0; }
  .rich :global(pre .bar) { display: flex; justify-content: space-between; align-items: center; padding: 3px 10px; border-bottom: 1px solid var(--line); color: var(--muted); font-family: var(--font, inherit); font-size: 12px; }
  .rich :global(pre .tools) { display: flex; align-items: center; }
  .rich :global(pre .run) { display: none; align-items: center; margin-right: 8px; border-radius: var(--r-sm); box-shadow: inset 0 0 0 1px var(--line-strong); }
  .canrun :global(pre .run) { display: inline-flex; }
  .rich :global(pre .run button) { background: none; border: 0; padding: 1px 8px; font: inherit; color: var(--text-2); cursor: pointer; display: inline-flex; align-items: center; gap: 4px; }
  .rich :global(pre .run button:hover) { background: var(--surface-hover); color: var(--text); }
  .rich :global(pre .run .go) { border-radius: var(--r-sm) 0 0 var(--r-sm); }
  .rich :global(pre .run .go svg) { color: var(--muted); }
  .rich :global(pre .run .more) { border-radius: 0 var(--r-sm) var(--r-sm) 0; border-left: 1px solid var(--line-strong); padding: 1px 4px; color: var(--muted); }
  .rich :global(pre .copy) { background: none; border: 0; padding: 0; color: var(--muted); font: inherit; cursor: pointer; }
  .rich :global(pre .copy:hover) { color: var(--text); }
  .rich :global(pre .copy.copied) { font-size: 0; }
  .rich :global(pre .copy.copied)::after { font-size: 12px; content: "Copied"; }
  .rich :global(pre code) { display: block; padding: 8px 12px; }
  .rich :global(th) { color: var(--muted); font-weight: 500; text-align: left; }
  .rich :global(td), .rich :global(th) { border: 0; border-bottom: 1px solid var(--line); padding: 4px 14px 4px 0; }
  .rich :global(.ask) { position: relative; margin-top: 0.9em; padding-left: 14px; color: var(--text); }
  .rich :global(.ask)::before { content: ""; position: absolute; left: 0; top: 0.3em; bottom: 0.3em; width: 2px; border-radius: 1px; background: var(--accent); }
</style>
