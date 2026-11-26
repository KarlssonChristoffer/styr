<!-- The project's library: what its threads made (pages, pictures, docs, files, servers), where each thing is now and what happened to it, read from the threads' logs by the core (`outputs.rs`). A list or a grid on the left, the picked thing on the right.
     Used by ProjectView.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  import Quiet from "./Quiet.svelte";
  // The project's library: what its threads made (pages, pictures, docs, files, servers), where
  // each thing is now and what happened to it, read from the threads' logs by the core
  // (`outputs.rs`). A list or a grid on the left, the picked thing on the right.
  import { api, pageUrl } from "../api";
  import type { Output } from "../bindings/Output";
  import type { FileText } from "../bindings/FileText";
  import { renderDoc } from "../docs";
  import { ago, clock } from "../format";
  import { openUrl } from "../native";
  import { prefs, setPref } from "../prefs.svelte";
  import { GLYPHS, shortPath, type Glyph } from "../refs";
  import { hub, unread } from "../state.svelte";
  import Icon from "./Icon.svelte";

  let { path, host = null }: { path: string; host?: string | null } = $props();

  let items = $state<Output[] | null>(null);
  let error = $state("");
  let kind = $state<string>("all");
  let query = $state("");
  let picked = $state<string | null>(null);

  const load = () =>
    api.projectOutputs(path, host).then((o) => {
      items = o;
      error = "";
      if (!picked || !o.some((i) => i.id === picked)) picked = o[0]?.id ?? null;
    }).catch((e) => (error = String(e?.message ?? e)));
  $effect(() => {
    void path;
    items = null;
    load();
    // Threads keep making things; read again now and then while the tab is open.
    const timer = poll(load, 20_000);
    return () => timer();
  });

  const KINDS: { id: string; label: string; glyph: Glyph | "image" | "server" }[] = [
    { id: "page", label: "Pages", glyph: "port" },
    { id: "picture", label: "Pictures", glyph: "image" },
    { id: "doc", label: "Docs", glyph: "doc" },
    { id: "file", label: "Files", glyph: "file" },
    { id: "server", label: "Running", glyph: "server" },
  ];
  const count = (k: string) => (items ?? []).filter((i) => i.kind === k).length;
  const shown = $derived(
    (items ?? []).filter(
      (i) => (kind === "all" || i.kind === kind) && (!query.trim() || `${i.title} ${i.thread_title ?? ""} ${i.places.map((p) => p.detail).join(" ")}`.toLowerCase().includes(query.trim().toLowerCase())),
    ),
  );
  const live = $derived(shown.filter((i) => i.live));
  const rest = $derived(shown.filter((i) => !i.live));
  const item = $derived((items ?? []).find((i) => i.id === picked) ?? null);
  const grid = $derived(prefs.libraryGrid);

  const KIND_NAME: Record<string, string> = { page: "Page", picture: "Picture", doc: "Doc", file: "File", server: "Server" };
  const PLACE_STATE: Record<string, string> = {
    live: "running now",
    kept: "here",
    committed: "committed",
    uncommitted: "changed, not committed",
    untracked: "not in git yet",
    gone: "gone",
  };

  function glyphOf(o: Output): string {
    if (o.kind === "picture") return "M3 5h18v14H3zM3 16l5-5 4 4 3-3 6 6M15 9h.01";
    if (o.kind === "page") return "M3 5h18v14H3zM3 9h18M7 7h.01";
    if (o.kind === "server") return GLYPHS.server;
    if (o.kind === "doc") return GLYPHS.doc;
    return GLYPHS.file;
  }
  function placeGlyph(kind: string): string {
    return kind === "repo" ? GLYPHS.repo : kind === "server" ? GLYPHS.server : kind === "hub" ? "M4 5h16v11H9l-5 4z" : GLYPHS.folder;
  }
  /** The main place in one line: where it is, and whether it is there. */
  function where(o: Output): { label: string; state: string; more: number } {
    const p = o.places[0];
    if (!p) return { label: "Nowhere now", state: "gone", more: 0 };
    return { label: p.kind === "folder" ? shortPath(p.detail) : p.label, state: p.state, more: o.places.length - 1 };
  }

  // Opening a thing keeps you in the library: it is shown big here, and back (Esc, the mouse's
  // back button) returns to the list. Hub's other places and the file manager only open from
  // their own buttons.
  function open(o: Output) {
    picked = o.id;
    hub.projectItem = o.id;
  }
  const close = () => (hub.projectItem = null);
  /** The doc in the project's Docs tab, when git has it there. */
  const docsRel = (o: Output) => (o.kind === "doc" ? (o.places.find((p) => p.kind === "repo")?.detail ?? null) : null);
  function readInDocs(o: Output) {
    hub.projectTab = "docs";
    hub.projectDoc = docsRel(o);
  }
  const reveal = (o: Output) => o.open.path && api.reveal(o.open.path).catch((e) => hub.fail(e));
  const gone = (o: Output) => o.places.length > 0 && o.places.every((p) => p.state === "gone");

  // The thing open big, and the next and previous in the list as it is filtered now.
  const reading = $derived(hub.projectItem ? ((items ?? []).find((i) => i.id === hub.projectItem) ?? null) : null);
  const order = $derived([...live, ...rest]);
  const at = $derived(reading ? order.findIndex((i) => i.id === reading.id) : -1);
  function stepTo(dir: -1 | 1) {
    if (!order.length) return;
    const from = at < 0 ? order.findIndex((i) => i.id === picked) : at;
    const next = order[Math.min(order.length - 1, Math.max(0, from + dir))];
    if (!next) return;
    if (reading) {
      picked = next.id;
      hub.projectItem = next.id;
    } else picked = next.id;
    document.querySelector(`[data-out="${CSS.escape(next.id)}"]`)?.scrollIntoView({ block: "nearest" });
  }

  // What is read for the thing open big: its text, or the picture's bytes.
  let text = $state<FileText | null>(null);
  let picture = $state<string | null>(null);
  let readError = $state("");
  const isMarkdown = (o: Output) => /\.(md|mdx|markdown)$/i.test(o.title);
  $effect(() => {
    const o = reading;
    text = null;
    readError = "";
    let url: string | null = null;
    let alive = true;
    if (o && !gone(o) && !o.open.page && !o.open.url) {
      if (o.kind === "picture") {
        api.outputPicture(path, o.id, host).then((u) => (alive ? (url = picture = u) : URL.revokeObjectURL(u))).catch((e) => alive && (readError = String(e?.message ?? e)));
      } else {
        api.outputText(path, o.id, host).then((t) => alive && (text = t)).catch((e) => alive && (readError = String(e?.message ?? e)));
      }
    }
    return () => {
      alive = false;
      picture = null;
      if (url) URL.revokeObjectURL(url);
    };
  });
  const html = $derived(reading && text && isMarkdown(reading) ? renderDoc(text.text, reading.title, []) : "");
  function clickInDoc(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    // Only a link out of the doc leaves Hub, and only because it was clicked.
    if (a.classList.contains("ext")) openUrl(a.getAttribute("href")!);
  }

  const typing = (el: EventTarget | null) =>
    el instanceof HTMLElement && (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName));
  function onKey(e: KeyboardEvent) {
    if (e.altKey || e.ctrlKey || e.metaKey || typing(e.target) || hub.viewing) return;
    if (reading) {
      if (e.key === "Escape" || e.key === "Backspace") close();
      else if (e.key === "ArrowLeft" || e.key === "k") stepTo(-1);
      else if (e.key === "ArrowRight" || e.key === "j") stepTo(1);
      else return;
    } else {
      if (e.key === "ArrowUp" || e.key === "k") stepTo(-1);
      else if (e.key === "ArrowDown" || e.key === "j") stepTo(1);
      else if (e.key === "Enter" && item) open(item);
      else return;
    }
    e.preventDefault();
  }
  const thread = (id: string | null) => (id ? hub.threads.find((t) => t.id === id) : undefined);
</script>

{#snippet dot(id: string | null)}
  {@const t = thread(id)}
  {#if t}<span class="dot {t.status}" class:unread={unread(t)}></span>{:else}<span class="dot idle"></span>{/if}
{/snippet}

{#snippet row(o: Output)}
  {@const w = where(o)}
  <button class="row" data-out={o.id} class:on={picked === o.id} onclick={() => open(o)}>
    <span class="name">
      <span class="thumb {o.kind}"><svg class="glyph" viewBox="0 0 24 24" aria-hidden="true"><path d={glyphOf(o)} /></svg></span>
      <span class="ellipsis">{o.title}</span>
      {#if o.kind === "page" && o.versions > 1}<span class="v">v{o.versions}</span>{/if}
    </span>
    <span class="by">{#if o.thread_title}{@render dot(o.thread)}<span class="ellipsis">{o.thread_title}</span>{:else}<span class="m">—</span>{/if}</span>
    <span class="where">
      <i class="state {w.state}"></i><span class="ellipsis" class:mono={o.places[0]?.kind === "folder"}>{w.label}</span>{#if w.more}<span class="m more">+{w.more}</span>{/if}
    </span>
    <span class="when">{o.live ? "now" : ago(o.changed_at)}</span>
    <span class="kind">{KIND_NAME[o.kind] ?? o.kind}{o.live ? " · live" : ""}</span>
  </button>
{/snippet}

{#snippet card(o: Output)}
  {@const w = where(o)}
  <button class="card" data-out={o.id} class:on={picked === o.id} onclick={() => open(o)}>
    <span class="pv {o.kind}">
      {#if o.kind === "page" && o.open.thread && o.open.page}
        <iframe title={o.title} src={pageUrl(o.open.thread, o.open.page)} sandbox="allow-scripts" loading="lazy" tabindex="-1"></iframe>
      {:else}
        <svg class="glyph big" viewBox="0 0 24 24" aria-hidden="true"><path d={glyphOf(o)} /></svg>
      {/if}
      {#if o.live}<span class="badge"><i class="state live"></i>live</span>{/if}
    </span>
    <span class="meta">
      <b class="ellipsis">{o.title}</b>
      <span class="ellipsis"><i class="state {w.state}"></i>{w.label}{o.live ? "" : ` · ${ago(o.changed_at)}`}</span>
    </span>
  </button>
{/snippet}

<svelte:window onkeydown={onKey} />

<div class="library">
  {#if reading}
    <div class="main reader">
      <div class="bar rbar">
        <button class="back" onclick={close} data-tip="Back to the library (Esc)"><svg class="arr" viewBox="0 0 24 24" aria-hidden="true"><path d="M15 6l-6 6 6 6" /></svg>Library</button>
        <span class="rt">
          <svg class="glyph" viewBox="0 0 24 24" aria-hidden="true"><path d={glyphOf(reading)} /></svg>
          <span class="ellipsis">{reading.title}</span>
        </span>
        {#if at >= 0}<span class="pos">{at + 1} of {order.length}</span>{/if}
        <span class="seg">
          <button disabled={at <= 0} data-tip="Previous (←)" aria-label="Previous" onclick={() => stepTo(-1)}><svg class="arr" viewBox="0 0 24 24" aria-hidden="true"><path d="M15 6l-6 6 6 6" /></svg></button>
          <button disabled={at < 0 || at >= order.length - 1} data-tip="Next (→)" aria-label="Next" onclick={() => stepTo(1)}><svg class="arr" viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6l6 6-6 6" /></svg></button>
        </span>
      </div>
      <div class="stage {reading.kind}">
        {#if reading.open.page && reading.open.thread}
          <iframe title={reading.title} src={pageUrl(reading.open.thread, reading.open.page)} sandbox="allow-scripts"></iframe>
        {:else if reading.open.url}
          <div class="say">
            <p>{reading.title} is running at <span class="mono">{reading.open.url}</span>.</p>
            <button class="btn small" onclick={() => openUrl(reading.open.url!)}>Open in the browser</button>
          </div>
        {:else if gone(reading)}
          <div class="say"><p>This is not there any more{reading.open.path ? "" : "."}{#if reading.open.path}: <span class="mono">{shortPath(reading.open.path)}</span> is gone.{/if}</p></div>
        {:else if readError}
          <div class="say"><p class="error">{readError}</p></div>
        {:else if reading.kind === "picture"}
          {#if picture}<img src={picture} alt={reading.title} />{:else}<div class="say"><Quiet kind="loading" /></div>{/if}
        {:else if !text}
          <div class="say"><Quiet kind="loading" /></div>
        {:else if html}
          <div role="presentation" class="doc" onclick={clickInDoc}>{@html html}</div>
        {:else}
          <pre class="txt">{text.text}</pre>
        {/if}
        {#if text?.truncated}<p class="cut">Only the start is shown; the file is larger.</p>{/if}
      </div>
    </div>
  {:else}
  <div class="main">
    <div class="bar">
      <button class="f" class:on={kind === "all"} onclick={() => (kind = "all")}>All {#if items}<span class="n">{items.length}</span>{/if}</button>
      {#each KINDS as k (k.id)}
        {#if count(k.id)}
          <button class="f" class:on={kind === k.id} onclick={() => (kind = kind === k.id ? "all" : k.id)}>{k.label} <span class="n">{count(k.id)}</span></button>
        {/if}
      {/each}
      <label class="search"><Icon name="search" size={13} /><input bind:value={query} placeholder="Find in what the threads made…" /></label>
      <span class="seg">
        <button class:on={grid} data-tip="Grid" onclick={() => setPref("libraryGrid", true)}><Icon name="card" size={13} /></button>
        <button class:on={!grid} data-tip="List" onclick={() => setPref("libraryGrid", false)}><Icon name="log" size={13} /></button>
      </span>
    </div>
    <div class="scroll">
      {#if error}
        <p class="none error">{error}</p>
      {:else if !items}
        <Quiet kind="loading" />
      {:else if !items.length}
        <Quiet text="Nothing made" />
      {:else if !shown.length}
        <Quiet text="Nothing matches" />
      {:else if grid}
        {#if live.length}<h4>Running now</h4><div class="grid">{#each live as o (o.id)}{@render card(o)}{/each}</div>{/if}
        {#if rest.length}{#if live.length}<h4>Made</h4>{/if}<div class="grid">{#each rest as o (o.id)}{@render card(o)}{/each}</div>{/if}
      {:else}
        <div class="head"><span>Name</span><span>Made by</span><span>Where</span><span>Changed</span><span>Kind</span></div>
        {#if live.length}<h4>Running now</h4>{#each live as o (o.id)}{@render row(o)}{/each}{/if}
        {#if rest.length}{#if live.length}<h4>Made</h4>{/if}{#each rest as o (o.id)}{@render row(o)}{/each}{/if}
      {/if}
    </div>
  </div>
  {/if}

  {#if item}
    <aside class="detail">
      {#if item.kind === "page" && item.open.thread && item.open.page}
        <button class="preview" onclick={() => open(item)} data-tip="Open big" aria-label="Open {item.title} big">
          <iframe title={item.title} src={pageUrl(item.open.thread, item.open.page)} sandbox="allow-scripts" tabindex="-1"></iframe>
        </button>
      {/if}
      <div class="dh">
        <svg class="glyph" viewBox="0 0 24 24" aria-hidden="true"><path d={glyphOf(item)} /></svg>
        <span class="ellipsis">{item.title}</span>
        {#if item.kind === "page" && item.versions > 1}<span class="v">v{item.versions}</span>{/if}
      </div>

      <h5>Where it is</h5>
      {#each item.places as p, i (i)}
        <div class="place" class:live={p.state === "live"} class:gone={p.state === "gone"}>
          <i class="state {p.state}"></i>
          <span class="pw">
            <span class="pl"><svg class="glyph" viewBox="0 0 24 24" aria-hidden="true"><path d={placeGlyph(p.kind)} /></svg>{p.label}</span>
            <span class="pd mono" data-tip={p.detail}>{p.kind === "folder" ? shortPath(p.detail) : p.detail}</span>
          </span>
          <span class="ps">{PLACE_STATE[p.state] ?? p.state}</span>
        </div>
      {/each}

      <h5>Life</h5>
      <div class="life">
        {#each item.life as s, i (i)}
          <div class:now={i === item.life.length - 1}>
            {#if s.thread && thread(s.thread)}
              <span>{s.what.replace(thread(s.thread)!.title, "").trim() || "In"}</span>
              <button class="tl" onclick={() => hub.open(s.thread!)}>{@render dot(s.thread)}<span class="ellipsis">{thread(s.thread)!.title}</span></button>
            {:else}
              <span>{s.what}</span>
            {/if}
            <span class="t">{item.live ? "" : Date.now() - s.at < 86_400_000 ? clock(s.at) : ago(s.at)}</span>
          </div>
        {/each}
      </div>

      {#if item.used_in.length}
        <h5>Also changed by</h5>
        <div class="uses">
          {#each item.used_in as u (u.thread)}
            <button class="tl" onclick={() => hub.open(u.thread)}>{@render dot(u.thread)}<span class="ellipsis">{u.title}</span></button>
          {/each}
        </div>
      {/if}

      <div class="acts">
        {#if reading?.id !== item.id}<button class="btn small primary" onclick={() => open(item)}>Open</button>{/if}
        {#if docsRel(item)}<button class="btn small" onclick={() => readInDocs(item)}>Read in Docs</button>{/if}
        {#if item.thread}<button class="btn small" onclick={() => hub.open(item.thread!)}>Open the thread</button>{/if}
        {#if item.open.path && !gone(item) && !host}<button class="btn small" onclick={() => reveal(item)}>Show in folder</button>{/if}
      </div>
    </aside>
  {/if}
</div>

<style>
  .library { display: flex; height: 100%; min-height: 0; }
  .main { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .bar { display: flex; align-items: center; gap: 6px; padding: 12px 0 10px; flex-wrap: wrap; }
  .f { display: inline-flex; align-items: center; gap: 6px; padding: 3px 8px; border-radius: var(--r-md); box-shadow: inset 0 0 0 1px var(--line); color: var(--text-3); font-size: 13px; }
  .f:hover { color: var(--text); }
  .f.on { background: var(--surface-2); box-shadow: inset 0 0 0 1px var(--line-strong); color: var(--text); }
  .f .n { color: var(--muted); font-size: 12px; }
  .search { margin-left: auto; display: flex; align-items: center; gap: 8px; padding: 4px 10px; border-radius: var(--r-md); box-shadow: inset 0 0 0 1px var(--line-strong); color: var(--muted); width: min(280px, 34%); }
  .search input { flex: 1; min-width: 0; background: none; border: 0; color: var(--text); font: inherit; font-size: 13px; }
  .search input:focus { outline: none; }
  .seg { display: flex; border-radius: var(--r-md); box-shadow: inset 0 0 0 1px var(--line); overflow: hidden; }
  .seg button { padding: 4px 8px; color: var(--muted); display: grid; place-items: center; }
  .seg button.on { background: var(--surface-2); color: var(--text); }
  .scroll { flex: 1; min-height: 0; overflow-y: auto; padding-bottom: 20px; }
  .none { color: var(--muted); font-size: 13px; line-height: 1.5; max-width: 60ch; }
  .none.error { color: var(--red); }
  h4 { margin: 14px 0 4px; color: var(--muted); font-size: 12px; font-weight: 500; }
  .head, .row { display: grid; grid-template-columns: minmax(0, 2.4fr) minmax(0, 1.4fr) minmax(0, 1.5fr) 80px 90px; gap: 14px; align-items: center; }
  .head { padding: 6px 10px; color: var(--muted); font-size: 12px; border-bottom: 1px solid var(--line); }
  .row { width: 100%; padding: 8px 10px; border-bottom: 1px solid var(--line); text-align: left; color: var(--text-2); font-size: 13px; }
  .row:hover { background: var(--surface); }
  .row.on { background: var(--surface-hover); }
  .name { display: flex; align-items: center; gap: 8px; min-width: 0; color: var(--text); }
  .thumb { width: 30px; height: 22px; flex: none; display: grid; place-items: center; border-radius: var(--r-xs); background: var(--surface-2); box-shadow: inset 0 0 0 1px var(--line-strong); }
  .thumb .glyph { margin: 0; }
  .v { flex: none; color: var(--muted); font-size: 11px; }
  .by, .where { display: flex; align-items: center; gap: 8px; min-width: 0; color: var(--text-3); }
  .where .more { flex: none; font-size: 12px; }
  .when, .kind { color: var(--muted); font-size: 12px; white-space: nowrap; }
  .mono { font-family: var(--mono); font-size: 0.92em; }
  i.state { width: 7px; height: 7px; border-radius: 50%; flex: none; display: inline-block; background: var(--line-strong); }
  i.state.live { background: var(--green); box-shadow: 0 0 0 3px var(--green-bg); }
  i.state.kept, i.state.committed { background: var(--text-3); }
  i.state.uncommitted, i.state.untracked { background: none; box-shadow: inset 0 0 0 1.5px var(--text-3); }
  i.state.gone { background: none; box-shadow: inset 0 0 0 1.5px var(--line-strong); }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(190px, 1fr)); gap: 12px; }
  .card { display: flex; flex-direction: column; border-radius: var(--r-lg); overflow: hidden; background: var(--surface); box-shadow: inset 0 0 0 1px var(--line); text-align: left; }
  .card:hover { box-shadow: inset 0 0 0 1px var(--line-strong); }
  .card.on { box-shadow: inset 0 0 0 1px var(--accent); }
  .pv { position: relative; height: 120px; display: grid; place-items: center; background: linear-gradient(180deg, var(--surface-hover), var(--surface)); overflow: hidden; }
  .pv iframe { position: absolute; left: 0; top: 0; width: 400%; height: 400%; transform: scale(0.25); transform-origin: 0 0; border: 0; pointer-events: none; }
  .glyph.big { width: 26px; height: 26px; margin: 0; stroke-width: 1.75; }
  .badge { position: absolute; right: 8px; top: 8px; display: inline-flex; align-items: center; gap: 4px; padding: 2px 8px; border-radius: var(--r-md); background: rgba(0, 0, 0, 0.6); color: var(--green); font-size: 11px; }
  .meta { display: flex; flex-direction: column; gap: 3px; padding: 8px 10px; min-width: 0; }
  .meta b { font-weight: 500; font-size: 13px; color: var(--text); }
  .meta span { display: flex; align-items: center; gap: 6px; color: var(--muted); font-size: 12px; }
  .detail { width: calc(380px * var(--text-grow)); flex: none; margin-left: 18px; padding: 14px 0 20px 18px; border-left: 1px solid var(--line); overflow-y: auto; display: flex; flex-direction: column; gap: 6px; }
  .preview { position: relative; height: 180px; border-radius: var(--r-lg); overflow: hidden; box-shadow: inset 0 0 0 1px var(--line-strong); background: var(--surface); margin-bottom: 6px; }
  .preview iframe { position: absolute; left: 0; top: 0; width: 250%; height: 250%; transform: scale(0.4); transform-origin: 0 0; border: 0; pointer-events: none; }
  .dh { display: flex; align-items: center; gap: 8px; font-size: 15px; font-weight: 600; color: var(--text); min-width: 0; }
  h5 { display: flex; align-items: center; gap: 8px; margin: 14px 0 4px; color: var(--muted); font-size: 12px; font-weight: 500; }
  h5::after { content: ""; flex: 1; height: 1px; background: var(--line); }
  .place { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border-radius: var(--r-md); background: var(--surface); box-shadow: inset 0 0 0 1px var(--line); }
  .place.live { box-shadow: inset 0 0 0 1px rgba(132, 194, 154, 0.35); }
  .place.gone { opacity: 0.7; }
  .pw { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .pl { display: flex; align-items: center; color: var(--text); font-size: 13px; }
  .pd { color: var(--muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ps { flex: none; color: var(--muted); font-size: 12px; text-align: right; }
  .live .ps { color: var(--green); }
  .life { position: relative; padding-left: 16px; }
  .life::before { content: ""; position: absolute; left: 4px; top: 8px; bottom: 8px; width: 1px; background: var(--line-strong); }
  .life > div { position: relative; display: flex; align-items: center; gap: 6px; padding: 3px 0; font-size: 13px; color: var(--text-3); min-width: 0; }
  .life > div::before { content: ""; position: absolute; left: -15px; top: 50%; margin-top: -4px; width: 7px; height: 7px; border-radius: 50%; background: var(--bg); box-shadow: inset 0 0 0 1.5px var(--muted); }
  .life > div.now::before { background: var(--text-3); box-shadow: none; }
  .life .t { margin-left: auto; flex: none; color: var(--muted); font-size: 12px; }
  .tl { display: inline-flex; align-items: center; gap: 6px; min-width: 0; max-width: 100%; padding: 1px 8px 1px 6px; border-radius: var(--r-sm); background: var(--surface-2); box-shadow: inset 0 0 0 1px var(--line); color: var(--text); font-size: 13px; }
  .tl:hover { background: var(--surface-hover); }
  .uses { display: flex; flex-wrap: wrap; gap: 6px; }
  .acts { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 14px; }
  .reader { min-height: 0; }
  .arr { width: 13px; height: 13px; fill: none; stroke: currentColor; stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
  .rbar { gap: 10px; flex-wrap: nowrap; }
  .back { display: inline-flex; align-items: center; gap: 6px; padding: 3px 8px 3px 6px; border-radius: var(--r-md); color: var(--text-3); font-size: 13px; box-shadow: inset 0 0 0 1px var(--line); }
  .back:hover { color: var(--text); background: var(--surface-hover); }
  .rt { display: flex; align-items: center; gap: 8px; min-width: 0; color: var(--text); font-size: 14px; font-weight: 500; }
  .pos { margin-left: auto; flex: none; color: var(--muted); font-size: 12px; }
  .seg button:disabled { opacity: 0.35; }
  .stage { flex: 1; min-height: 0; overflow: auto; border-radius: var(--r-lg); background: var(--surface); box-shadow: inset 0 0 0 1px var(--line); margin-bottom: 14px; position: relative; }
  .stage.page { overflow: hidden; }
  .stage iframe { width: 100%; height: 100%; border: 0; display: block; }
  .stage.picture { display: grid; place-items: center; background: var(--bg); }
  .stage img { max-width: 100%; max-height: 100%; object-fit: contain; display: block; }
  .say { padding: 24px; color: var(--text-3); font-size: 13px; display: flex; flex-direction: column; align-items: flex-start; gap: 10px; }
  .say p { margin: 0; }
  .say .error { color: var(--red); }
  .txt { margin: 0; padding: 18px 20px; font-family: var(--mono); font-size: 13px; line-height: 1.55; color: var(--text-2); white-space: pre-wrap; word-break: break-word; }
  .doc { padding: 22px 28px; max-width: 78ch; color: var(--text-2); font-size: 14px; line-height: 1.6; }
  .doc :global(p) { margin: 0 0 0.8em; }
  .doc :global(h1), .doc :global(h2), .doc :global(h3), .doc :global(h4) { color: var(--text); font-weight: 500; line-height: 1.3; }
  .doc :global(h1) { font-size: 20px; margin: 1.4em 0 0.5em; }
  .doc :global(h2) { font-size: 16px; margin: 1.4em 0 0.4em; }
  .doc :global(h3) { font-size: 15px; margin: 1.2em 0 0.3em; }
  .doc :global(> :first-child) { margin-top: 0; }
  .doc :global(a) { color: var(--accent); }
  .doc :global(code) { font-family: var(--mono); font-size: 0.9em; background: var(--surface-2); padding: 1px 4px; border-radius: var(--r-xs); }
  .doc :global(pre) { overflow-x: auto; }
  .doc :global(pre code) { background: none; padding: 0; }
  .doc :global(ul), .doc :global(ol) { padding-left: 1.4em; margin: 0 0 0.8em; }
  .doc :global(table) { border-collapse: collapse; margin: 0 0 1em; }
  .doc :global(th), .doc :global(td) { border: 1px solid var(--line); padding: 4px 8px; }
  .doc :global(blockquote) { margin: 0 0 0.8em; padding-left: 12px; border-left: 2px solid var(--line-strong); color: var(--text-3); }
  .cut { margin: 0; padding: 8px 20px 14px; color: var(--muted); font-size: 12px; }
  .ellipsis { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
</style>
