<!-- The project's markdown as linked docs, the way Obsidian reads a folder: a tree and the docs changed lately on the left, the doc in the middle with its [[links]] live, and what is on the page, what links here and where it links on the right. Ctrl O finds any doc. `version` changes when the project page reloads, as agents edit the docs while they work. `doc` is the doc on screen (a path or name, with `#heading`), and `ongo` moves to another, so back and forward step through docs. The library uses the same reader without the tree (`nav`), with its own buttons beside the title (`actions`).
     Used by LibraryView.svelte, ProjectView.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import Quiet from "./Quiet.svelte";
  import type { Snippet } from "svelte";
  import { api } from "../api";
  import type { DocEntry } from "../bindings/DocEntry";
  import type { DocPage } from "../bindings/DocPage";
  import type { VaultMap } from "../bindings/VaultMap";
  import { plainHeading, renderDoc, slug, splitTarget } from "../docs";
  import { ago } from "../format";
  import { copy, menu } from "../menu.svelte";
  import { portal } from "../portal";
  import { fileMenu, join } from "../menus.svelte";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";
  import DocSwitcher from "./DocSwitcher.svelte";
  import Icon from "./Icon.svelte";

  // The project's markdown as linked docs, the way Obsidian reads a folder: a tree and the docs
  // changed lately on the left, the doc in the middle with its [[links]] live, and what is on
  // the page, what links here and where it links on the right. Ctrl O finds any doc.
  // `version` changes when the project page reloads, as agents edit the docs while they work.
  // `doc` is the doc on screen (a path or name, with `#heading`), and `ongo` moves to another, so
  // back and forward step through docs. The library uses the same reader without the tree
  // (`nav`), with its own buttons beside the title (`actions`).
  let {
    path,
    host = null,
    version,
    doc,
    ongo,
    nav = true,
    empty = "No markdown in this project yet. Docs agents write in <code>docs/</code> show up here.",
    actions,
  }: {
    path: string;
    host?: string | null;
    version: unknown;
    doc: string | null;
    ongo: (target: string) => void;
    nav?: boolean;
    empty?: string;
    actions?: Snippet<[DocPage]>;
  } = $props();

  let vault = $state<VaultMap | null>(null);
  let page = $state<DocPage | null>(null);
  let missing = $state<string | null>(null);
  let finding = $state<string | null>(null);

  // Saving a doc reads it all again.
  let saved = $state(0);
  $effect(() => {
    void version;
    void saved;
    api.vault(path, host).then((v) => (vault = v)).catch((e) => hub.fail(e));
  });

  // The doc on screen is part of where you are, so back and forward step through docs too.
  const wanted = $derived(doc ? splitTarget(doc) : null);
  const first = (v: VaultMap) =>
    v.docs.find((d) => ["readme.md", "index.md"].includes(d.rel.toLowerCase())) ?? [...v.docs].sort((a, b) => a.rel.localeCompare(b.rel))[0];
  $effect(() => {
    void version;
    void saved;
    const target = wanted?.doc ?? (vault ? first(vault)?.rel : undefined);
    if (!target) return;
    api
      .vaultDoc(path, target, host)
      .then((p) => {
        if ((wanted?.doc ?? target) !== target) return;
        const moved = p.rel !== page?.rel;
        page = p;
        missing = null;
        if (moved) jump(wanted?.heading ?? null);
      })
      .catch(() => {
        missing = target;
        page = null;
      });
  });

  function go(doc: string, heading: string | null = null) {
    if (editing && !leave()) return;
    ongo(heading ? `${doc}#${heading}` : doc);
    if (page && doc === page.rel) jump(heading);
  }

  // ---- Editing ----
  // The doc's file as it is, in a plain text box. Typing [[ offers the docs to link to.
  let editing = $state<{ rel: string; text: string; was: string } | null>(null);
  let box = $state<HTMLTextAreaElement>();
  let linking = $state<{ from: number; query: string; at: number } | null>(null);
  const changed = $derived(!!editing && editing.text !== editing.was);
  async function edit() {
    if (!page) return;
    try {
      const file = await api.projectFile(path, page.rel, host);
      if (file.truncated) return hub.fail(new Error("This doc is too big to edit here. Open it in your editor."));
      editing = { rel: page.rel, text: file.text, was: file.text };
      queueMicrotask(() => box?.focus());
    } catch (e) {
      hub.fail(e);
    }
  }
  async function save() {
    if (!editing) return;
    try {
      await api.saveDoc(path, editing.rel, editing.text, host);
      editing = null;
      linking = null;
      previews.clear();
      saved++;
    } catch (e) {
      hub.fail(e);
    }
  }
  /** Leaves the editor, unless there are changes the user wants to keep. */
  function leave() {
    if (changed && !confirm("Throw away your changes to this doc?")) return false;
    editing = null;
    linking = null;
    return true;
  }
  const choices = $derived.by(() => {
    if (!linking || !vault) return [];
    const q = linking.query.toLowerCase();
    return vault.docs
      .filter((d) => !q || d.title.toLowerCase().includes(q) || d.rel.toLowerCase().includes(q))
      .sort((a, b) => Number(!a.title.toLowerCase().startsWith(q)) - Number(!b.title.toLowerCase().startsWith(q)) || b.links_in - a.links_in)
      .slice(0, 8);
  });
  /** After each keystroke: is the caret inside an open `[[`? */
  function typed() {
    if (!box || !editing) return;
    const caret = box.selectionStart;
    const before = editing.text.slice(0, caret);
    const open = before.lastIndexOf("[[");
    const inside = open >= 0 ? before.slice(open + 2) : null;
    linking = inside !== null && !/[\]\n|]/.test(inside) ? { from: open + 2, query: inside, at: linking?.query === inside ? linking.at : 0 } : null;
  }
  function pick(d: DocEntry) {
    if (!editing || !linking || !box) return;
    const caret = box.selectionStart;
    const name = d.rel.split("/").pop()!.replace(/\.md$/i, "");
    const after = editing.text.slice(caret);
    const close = after.startsWith("]]") ? "" : "]]";
    editing.text = editing.text.slice(0, linking.from) + name + close + after;
    const end = linking.from + name.length + 2;
    linking = null;
    queueMicrotask(() => {
      box?.focus();
      box?.setSelectionRange(end, end);
    });
  }
  function keys(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      save();
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      if (linking) linking = null;
      else leave();
      return;
    }
    if (!linking || !choices.length) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const n = choices.length;
      linking.at = (linking.at + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
    } else if (e.key === "Enter" || e.key === "Tab") {
      e.preventDefault();
      pick(choices[linking.at]);
    }
  }

  let body = $state<HTMLElement>();
  // The doc scrolls in its own column, so the tree and the links beside it stay where they are.
  let reader = $state<HTMLElement>();
  function jump(heading: string | null) {
    queueMicrotask(() => {
      const el = heading ? body?.querySelector<HTMLElement>(`#h-${CSS.escape(slug(heading))}`) : null;
      if (el) el.scrollIntoView({ block: "start", behavior: "smooth" });
      else reader?.scrollTo({ top: 0 });
    });
  }

  // The title is above the text, so a first heading that says the same is left out.
  const html = $derived.by(() => {
    if (!page) return "";
    const first = /^#\s+(.+)\r?\n?/.exec(page.body);
    const body = first && plainHeading(first[1]).toLowerCase() === page.title.toLowerCase() ? page.body.slice(first[0].length) : page.body;
    return renderDoc(body, page.rel, page.links);
  });

  function click(e: MouseEvent) {
    const copyButton = (e.target as HTMLElement).closest<HTMLButtonElement>(".cb-copy");
    if (copyButton) {
      const code = copyButton.closest(".codeblock")?.querySelector("code")?.textContent ?? "";
      copy(code)
        .then(() => {
          copyButton.textContent = "Copied";
          setTimeout(() => (copyButton.textContent = "Copy"), 1500);
        })
        .catch((err) => hub.fail(err));
      return;
    }
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    hover = null;
    if (a.dataset.doc) {
      const { doc, heading } = splitTarget(a.dataset.doc);
      if (a.classList.contains("miss")) {
        hub.fail(new Error(`${doc} is not written yet. Ask an agent to write it, and it shows up here.`));
        return;
      }
      go(doc, heading);
    } else if (a.dataset.tag) {
      finding = `#${a.dataset.tag}`;
    } else {
      const href = a.getAttribute("href") ?? "";
      if (href.startsWith("#")) jump(href.slice(1));
      else if (href) openUrl(href);
    }
  }

  // The page body is rendered HTML whose links and copy buttons are real, keyboard-reachable elements; one listener on the container serves them all.
  function delegate(node: HTMLElement) {
    const leave = () => (hover = null);
    node.addEventListener("click", click);
    node.addEventListener("mouseover", over);
    node.addEventListener("mouseleave", leave);
    return {
      destroy() {
        node.removeEventListener("click", click);
        node.removeEventListener("mouseover", over);
        node.removeEventListener("mouseleave", leave);
      },
    };
  }

  // Hovering a link shows where it leads before you go.
  let hover = $state<{ x: number; y: number; above: boolean; doc: DocPage | null; target: string } | null>(null);
  const previews = new Map<string, Promise<DocPage | null>>();
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  function over(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest<HTMLElement>("a.wl[data-doc]:not(.miss)");
    clearTimeout(hoverTimer);
    if (!a) {
      hoverTimer = setTimeout(() => (hover = null), 150);
      return;
    }
    const { doc } = splitTarget(a.dataset.doc!);
    hoverTimer = setTimeout(async () => {
      if (!previews.has(doc)) previews.set(doc, quiet(api.vaultDoc(path, doc, host), "vault doc", null));
      const r = a.getBoundingClientRect();
      const found = await previews.get(doc)!;
      // Below the link, or above it when the bottom of the window is near.
      const above = r.bottom > window.innerHeight - 260;
      const x = Math.min(r.left, window.innerWidth - 400);
      if (a.isConnected && a.matches(":hover")) hover = { x, y: above ? r.top - 6 : r.bottom + 6, above, doc: found, target: doc };
    }, 350);
  }
  $effect(() => {
    void version;
    previews.clear();
  });

  // ---- The tree ----

  type Folder = { name: string; rel: string; folders: Folder[]; docs: DocEntry[]; count: number };
  const tree = $derived.by(() => {
    const root: Folder = { name: "", rel: "", folders: [], docs: [], count: 0 };
    for (const d of vault?.docs ?? []) {
      const parts = d.rel.split("/");
      let at = root;
      at.count++;
      for (const part of parts.slice(0, -1)) {
        const rel = at.rel ? `${at.rel}/${part}` : part;
        let next = at.folders.find((f) => f.name === part);
        if (!next) at.folders.push((next = { name: part, rel, folders: [], docs: [], count: 0 }));
        next.count++;
        at = next;
      }
      at.docs.push(d);
    }
    const sort = (f: Folder) => {
      f.folders.sort((a, b) => a.name.localeCompare(b.name));
      f.docs.sort((a, b) => a.title.localeCompare(b.title));
      f.folders.forEach(sort);
    };
    sort(root);
    return root;
  });
  // Folders you opened or closed; the rest are open when they hold the doc on screen or are docs/.
  let toggled = $state<Record<string, boolean>>({});
  const isOpen = (f: Folder) => toggled[f.rel] ?? (f.rel === "docs" || !!page?.rel.startsWith(`${f.rel}/`));

  const pinned = $derived(
    (vault?.docs ?? []).filter((d) => !d.rel.includes("/") || d.rel.toLowerCase() === "docs/plan.md").sort((a, b) => a.rel.localeCompare(b.rel)),
  );
  const DAY = 86_400_000;
  const recent = $derived((vault?.docs ?? []).filter((d) => Date.now() - d.modified < 7 * DAY).slice(0, 6));

  const crumbs = $derived(page ? page.rel.split("/").slice(0, -1) : []);
  const outline = $derived(page?.headings.filter((h) => h.level <= 3) ?? []);
  const linksTo = $derived.by(() => {
    const seen = new Set<string>();
    return (page?.links ?? []).filter((l) => {
      const key = l.to ?? `?${l.target}`;
      if (l.to === page?.rel || seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  });
  const titleOf = (rel: string) => vault?.docs.find((d) => d.rel === rel)?.title ?? rel.split("/").pop()!.replace(/\.md$/i, "");
  const hiddenProps = new Set(["title", "summary", "description", "tags", "tag", "understood"]);
  const shownProps = $derived(page?.props.filter((p) => !hiddenProps.has(p.key.toLowerCase()) && p.value) ?? []);

  // Ctrl O finds a doc from anywhere on the tab.
  $effect(() => {
    const key = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "o") {
        e.preventDefault();
        finding = finding === null ? "" : null;
      }
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  });

  // A line of context with its [[links]] read as their names.
  const plain = (s: string) => s.replace(/\[\[([^\]|#]*)(#[^\]|]*)?\|?([^\]]*)\]\]/g, (_, t, h, a) => a || t || h?.slice(1) || "");
</script>

{#snippet folder(f: Folder, depth: number)}
  {#each f.folders as sub (sub.rel)}
    <button class="row" style:padding-left="{6 + depth * 14}px" onclick={() => (toggled[sub.rel] = !isOpen(sub))}>
      <span class="chev" class:open={isOpen(sub)}><Icon name="chevron" size={11} /></span>
      <span class="name">{sub.name}</span><span class="c">{sub.count}</span>
    </button>
    {#if isOpen(sub)}{@render folder(sub, depth + 1)}{/if}
  {/each}
  {#each f.docs as d (d.rel)}
    <button
      class="row doc"
      class:active={page?.rel === d.rel}
      style:padding-left="{6 + depth * 14 + 16}px"
      data-tip={d.summary}
      onclick={() => go(d.rel)}
      use:menu={() => fileMenu(join(path, d.rel), { rel: d.rel, open: () => go(d.rel) })}
    >
      <span class="name">{d.title}</span>
    </button>
  {/each}
{/snippet}

<div class="docs-tab" class:bare={!nav}>
  {#if nav}
  <aside class="pane nav">
    <button class="find" onclick={() => (finding = "")}>
      <Icon name="search" size={13} /><span>Find a doc</span><kbd>Ctrl O</kbd>
    </button>
    {#if vault && !vault.docs.length}
      <p class="none">{@html empty}</p>
    {/if}
    {#if pinned.length}
      <div class="label">Start</div>
      {#each pinned as d (d.rel)}
        <button class="row doc" class:active={page?.rel === d.rel} onclick={() => go(d.rel)} use:menu={() => fileMenu(join(path, d.rel), { rel: d.rel, open: () => go(d.rel) })}>
          <Icon name={d.rel.toLowerCase() === "docs/plan.md" ? "plan" : "file"} size={12} /><span class="name">{d.title}</span>
        </button>
      {/each}
    {/if}
    {#if tree.folders.length}
      <div class="label">Folders</div>
      {@render folder({ ...tree, docs: [] }, 0)}
    {/if}
    {#if recent.length}
      <div class="label">Changed lately</div>
      {#each recent as d (d.rel)}
        <button class="row doc" class:active={page?.rel === d.rel} onclick={() => go(d.rel)}>
          <span class="name">{d.title}</span><span class="c">{ago(d.modified)}</span>
        </button>
      {/each}
    {/if}
    {#if vault?.tags.length}
      <div class="label">Tags</div>
      <div class="tags">
        {#each vault.tags.slice(0, 24) as t (t.tag)}
          <button class="tag" onclick={() => (finding = `#${t.tag}`)}>#{t.tag}<span>{t.count}</span></button>
        {/each}
      </div>
    {/if}
    {#if vault?.skipped}<p class="none">{vault.skipped} files left out.</p>{/if}
  </aside>
  {/if}

  <article class="pane reader" bind:this={reader}>
    {#if editing}
      <div class="crumbs mono">
        <span>Editing {editing.rel}</span>
        <span class="when">
          <button class="act" onclick={leave}>Cancel</button>
          <button class="act primary" disabled={!changed} onclick={save}>Save <kbd>Ctrl S</kbd></button>
        </span>
      </div>
      <div class="editor">
        <textarea bind:this={box} bind:value={editing.text} spellcheck="false" oninput={typed} onkeydown={keys} onclick={typed}></textarea>
        {#if linking && choices.length}
          <div class="links">
            <div class="lh">Link to <kbd>↑↓</kbd> <kbd>Enter</kbd></div>
            {#each choices as d, i (d.rel)}
              <button class="choice" class:on={i === linking.at} onmousedown={(e) => { e.preventDefault(); pick(d); }}>
                <span class="t">{d.title}</span><span class="r mono">{d.rel}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {:else if page}
      <div class="crumbs mono">
        {#each crumbs as c, i (i)}<span>{c}</span><span class="sep">›</span>{/each}<span>{page.rel.split("/").pop()}</span>
        <span class="when">
          {ago(page.modified)}
          {#if actions}{@render actions(page)}{/if}
          <button class="act" onclick={edit} data-tip="Edit this doc. Type [[ to link another.">Edit</button>
        </span>
      </div>
      <h1 use:menu={() => fileMenu(join(path, page!.rel), { rel: page!.rel })}>{page.title}</h1>
      {#if shownProps.length || page.tags.length}
        <div class="props">
          {#each shownProps as p (p.key)}<span class="prop"><b>{p.key}</b> {p.value}</span>{/each}
          {#each page.tags as t (t)}<button class="tag" onclick={() => (finding = `#${t}`)}>#{t}</button>{/each}
        </div>
      {/if}
      <div class="body" bind:this={body} use:delegate>
        {@html html}
      </div>
      {#if page.truncated}<p class="none">Too big to show.</p>{/if}
    {:else if missing}
      <p class="none">No doc called <b>{missing}</b>.</p>
    {:else if vault?.docs.length}
      <Quiet kind="loading" />
    {/if}
  </article>

  <aside class="side">
    {#if page && outline.length > 1}
      <section class="pane">
        <h3>On this page</h3>
        {#each outline as h, i (i)}
          <button class="out" style:padding-left="{(h.level - 1) * 12 + 6}px" onclick={() => jump(h.text)}>{h.text}</button>
        {/each}
      </section>
    {/if}
    {#if page}
      <section class="pane">
        <h3>Linked from <span class="n">{page.backlinks.length}</span></h3>
        {#each page.backlinks as b (b.from)}
          <button class="bl" onclick={() => go(b.from)}>
            <span class="t">{b.title}</span>
            {#if b.context}<span class="s">{plain(b.context)}</span>{/if}
          </button>
        {:else}
          <Quiet text="Nothing links here" />
        {/each}
      </section>
      {#if page.mentions.length}
        <section class="pane">
          <h3>Named without a link <span class="n">{page.mentions.length}</span></h3>
          {#each page.mentions as m (m.from)}
            <button class="bl" onclick={() => go(m.from)}>
              <span class="t">{m.title}</span><span class="s">{plain(m.context)}</span>
            </button>
          {/each}
        </section>
      {/if}
      {#if linksTo.length}
        <section class="pane">
          <h3>Links to <span class="n">{linksTo.length}</span></h3>
          {#each linksTo as l (l.to ?? `?${l.target}`)}
            {#if l.to}
              <button class="out" onclick={() => go(l.to!, l.heading)}>{titleOf(l.to)}</button>
            {:else}
              <span class="out miss" data-tip="Not written yet">{l.target}</span>
            {/if}
          {/each}
        </section>
      {/if}
    {/if}
  </aside>
</div>

{#if hover}
  <div class="preview" use:portal class:above={hover.above} style:left="{hover.x}px" style:top="{hover.y}px">
    {#if hover.doc}
      <div class="t">{hover.doc.title}</div>
      <div class="m mono">{hover.doc.rel} · {ago(hover.doc.modified)}</div>
      <p>{hover.doc.summary}</p>
    {:else}
      <div class="m">No doc called {hover.target}</div>
    {/if}
  </div>
{/if}

{#if finding !== null}
  <DocSwitcher {path} {host} query={finding} docs={vault?.docs ?? []} onpick={(rel, heading) => { finding = null; go(rel, heading); }} onclose={() => (finding = null)} />
{/if}

<style>
  /* Three columns as tall as the tab, each scrolling on its own, as in Obsidian. */
  .docs-tab { display: grid; grid-template-columns: 250px minmax(0, 1fr) 290px; gap: 14px; height: 100%; min-height: 0; }
  .docs-tab > * { min-height: 0; overflow-y: auto; }
  .docs-tab.bare { grid-template-columns: minmax(0, 1fr) 290px; }
  @media (max-width: 1250px) { .docs-tab.bare { grid-template-columns: minmax(0, 1fr); } }
  @media (min-width: 2200px) { .docs-tab { grid-template-columns: 280px minmax(0, 1fr) 340px; } }
  @media (max-width: 1250px) { .docs-tab { grid-template-columns: 220px minmax(0, 1fr); } .side { display: none; } }
  @media (max-width: 860px) { .docs-tab { grid-template-columns: minmax(0, 1fr); overflow-y: auto; } .docs-tab > * { overflow: visible; } .nav { display: none; } }
  .pane { background: var(--surface); border-radius: var(--r-lg); min-width: 0; }
  .nav { padding: 8px; }
  .find { display: flex; align-items: center; gap: 8px; width: 100%; background: var(--surface-2); border-radius: var(--r-md); padding: 6px 8px; color: var(--muted); font-size: 13px; margin-bottom: 4px; }
  .find:hover { color: var(--text-2); }
  .find span { flex: 1; text-align: left; }
  kbd { font: 11px var(--mono); border: 1px solid var(--line-strong); border-radius: var(--r-xs); padding: 0 4px; color: var(--muted); }
  .label { color: var(--muted); font-size: 12px; margin: 12px 6px 3px; }
  .row { display: flex; align-items: center; gap: 6px; width: 100%; height: 26px; padding-right: 6px; padding-left: 6px; border-radius: var(--r-sm); color: var(--text-3); font-size: 13px; text-align: left; }
  .row:hover { background: var(--surface-hover); color: var(--text-2); }
  .row.active { background: var(--surface-hover); color: var(--text); }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .c { color: var(--muted); font-size: 11px; flex: none; }
  .chev { color: var(--muted); display: inline-flex; transition: transform 0.12s var(--ease); flex: none; }
  .chev.open { transform: rotate(90deg); }
  .tags { display: flex; flex-wrap: wrap; gap: 4px; padding: 0 6px 6px; }
  .tag { color: var(--accent); background: var(--accent-bg); border-radius: var(--r-sm); padding: 0 6px; font-size: 12px; line-height: 1.6; }
  .tag span { color: var(--muted); margin-left: 4px; font-size: 11px; }
  .none { color: var(--muted); font-size: 13px; margin: 6px; line-height: 1.5; }

  .reader { padding: 18px 34px 48px; mask-image: linear-gradient(to bottom, #000 calc(100% - 28px), transparent); }
  .crumbs { display: flex; align-items: center; gap: 6px; color: var(--muted); font-size: 12px; flex-wrap: wrap; }
  .crumbs .sep { opacity: 0.6; }
  .crumbs .when { margin-left: auto; font-family: var(--font); display: flex; align-items: center; gap: 6px; }
  .act { color: var(--text-3); border: 1px solid var(--line-strong); border-radius: var(--r-sm); padding: 1px 8px; font: 12px var(--font); }
  .act:hover:not(:disabled) { color: var(--text); background: var(--surface-hover); }
  .act:disabled { opacity: 0.5; }
  .act.primary:not(:disabled) { color: var(--text); border-color: var(--accent); }
  .act kbd { margin-left: 4px; }
  .editor { position: relative; margin-top: 10px; height: calc(100% - 40px); min-height: 360px; }
  textarea { width: 100%; height: 100%; resize: none; background: var(--surface-2); color: var(--text-2); border: 1px solid var(--line); border-radius: var(--r-md); padding: 12px 14px; font: 13px/1.65 var(--mono); outline: none; }
  textarea:focus { border-color: var(--line-strong); }
  .links { position: absolute; right: 12px; top: 12px; width: min(460px, calc(100% - 24px)); background: var(--surface); border: 1px solid var(--line-strong); border-radius: var(--r-lg); padding: 6px; box-shadow: 0 12px 32px rgb(0 0 0 / 0.4); }
  .lh { color: var(--muted); font-size: 11px; margin: 2px 6px 4px; }
  .choice { display: flex; gap: 10px; align-items: baseline; width: 100%; text-align: left; padding: 4px 8px; border-radius: var(--r-sm); }
  .choice.on, .choice:hover { background: var(--surface-hover); }
  .choice .t { color: var(--text-2); font-size: 13px; }
  .choice .r { color: var(--muted); font-size: 11px; margin-left: auto; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  h1 { color: var(--text); font-size: 24px; font-weight: 500; letter-spacing: -0.01em; margin: 8px 0 6px; line-height: 1.25; }
  .props { display: flex; flex-wrap: wrap; gap: 6px; margin: 4px 0 14px; }
  .prop { background: var(--surface-2); border-radius: var(--r-sm); padding: 1px 8px; font-size: 12px; color: var(--text-3); }
  .prop b { font-weight: 400; color: var(--muted); }

  .body { user-select: text; -webkit-user-select: text; font-size: 14px; line-height: 1.7; color: var(--text-2); overflow-wrap: anywhere; }
  .body :global(p) { margin: 0 0 0.8em; }
  .body :global(h1), .body :global(h2), .body :global(h3), .body :global(h4) { color: var(--text); font-weight: 500; line-height: 1.3; scroll-margin-top: 12px; }
  .body :global(h1) { font-size: 20px; margin: 1.4em 0 0.5em; }
  .body :global(h2) { font-size: 16px; margin: 1.4em 0 0.4em; }
  .body :global(h3) { font-size: 15px; margin: 1.2em 0 0.3em; }
  .body :global(h4) { font-size: 14px; margin: 1em 0 0.3em; }
  .body :global(> :first-child) { margin-top: 0; }
  .body :global(a.wl) { color: var(--accent); text-decoration: underline dotted; text-underline-offset: 3px; }
  .body :global(a.wl.miss) { color: var(--muted); text-decoration-style: dashed; cursor: help; }
  .body :global(a.ext) { color: var(--accent); }
  .body :global(a.tag) { text-decoration: none; }
  .body :global(code) { background: var(--surface-2); padding: 1px 4px; border-radius: var(--r-xs); font: 13px var(--mono); }
  .body :global(.codeblock) { background: var(--bg-sidebar); border: 1px solid var(--line); border-radius: var(--r-md); margin: 0 0 1em; overflow: hidden; }
  .body :global(.cb-head) { display: flex; align-items: center; gap: 10px; padding: 4px 8px 4px 12px; border-bottom: 1px solid var(--line); font-size: 12px; color: var(--muted); }
  .body :global(.cb-lang) { color: var(--text-3); }
  .body :global(.cb-copy) { margin-left: auto; color: var(--muted); padding: 1px 8px; border-radius: var(--r-sm); font-size: 12px; }
  .body :global(.cb-copy:hover) { color: var(--text); background: var(--surface-hover); }
  .body :global(.codeblock pre) { margin: 0; padding: 10px 12px; overflow-x: auto; }
  .body :global(pre code) { background: none; padding: 0; font-size: 13px; line-height: 1.6; }
  .body :global(ul), .body :global(ol) { padding-left: 1.4em; margin: 0 0 0.8em; }
  .body :global(li > input[type="checkbox"]) { margin: 0 6px 0 -1.2em; vertical-align: -1px; }
  .body :global(li:has(> input[type="checkbox"])) { list-style: none; }
  .body :global(blockquote) { border-left: 2px solid var(--line-strong); margin: 0 0 0.8em; padding: 2px 14px; color: var(--text-3); }
  .body :global(table) { border-collapse: collapse; margin: 0 0 0.8em; font-size: 13px; }
  .body :global(td), .body :global(th) { border: 1px solid var(--line); padding: 4px 8px; text-align: left; }
  .body :global(hr) { border: 0; border-top: 1px solid var(--line); margin: 1.4em 0; }
  .body :global(img) { max-width: 100%; border-radius: var(--r-md); }

  .side { display: flex; flex-direction: column; gap: 12px; }
  .side .pane { padding: 10px 8px; }
  h3 { color: var(--text); font-size: 13px; font-weight: 500; margin: 2px 6px 6px; }
  .n { color: var(--muted); font-weight: 400; font-size: 12px; margin-left: 2px; }
  .out { display: block; width: 100%; text-align: left; padding: 3px 6px; border-radius: var(--r-sm); color: var(--text-3); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button.out:hover { background: var(--surface-hover); color: var(--text); }
  .out.miss { color: var(--muted); text-decoration: underline dashed; text-underline-offset: 3px; }
  .bl { display: flex; flex-direction: column; align-items: flex-start; gap: 1px; width: 100%; text-align: left; padding: 4px 6px; border-radius: var(--r-sm); }
  .bl:hover { background: var(--surface-hover); }
  .bl .t { color: var(--text-2); font-size: 13px; }
  .bl .s { color: var(--muted); font-size: 12px; line-height: 1.45; display: -webkit-box; -webkit-line-clamp: 3; line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }

  .preview { position: fixed; z-index: 40; width: 380px; background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg); padding: 11px 14px; box-shadow: 0 12px 32px rgb(0 0 0 / 0.4); pointer-events: none; }
  .preview.above { transform: translateY(-100%); }
  .preview .t { color: var(--text); font-weight: 500; }
  .preview .m { color: var(--muted); font-size: 12px; }
  .preview p { margin: 6px 0 0; color: var(--text-3); font-size: 13px; line-height: 1.55; }
</style>
