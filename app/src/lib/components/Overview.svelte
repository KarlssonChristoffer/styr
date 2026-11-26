<!-- Overview: looking back across every project. Search what you did, then a board of cards you pick, place and size yourself (Customize): the activity map and the day it picks, the devlog, project health, models and cost, and the playful ones. The board itself is `Board.svelte`, shared with Home (overview.svelte.ts holds the list, board.ts the arithmetic).
     Used by App.svelte. -->
<script lang="ts">
  // Overview: looking back across every project. Search what you did, then a board of cards you
  // pick, place and size yourself (Customize): the activity map and the day it picks, the devlog,
  // project health, models and cost, and the playful ones. The board itself is `Board.svelte`,
  // shared with Home (overview.svelte.ts holds the list, board.ts the arithmetic).
  import { api } from "../api";
  import type { HistoryHit } from "../bindings/HistoryHit";
  import { folderName } from "../format";
  import { pickDay } from "../overview.svelte";
  import { hub } from "../state.svelte";
  import Board from "./Board.svelte";
  import CardBody from "./CardBody.svelte";
  import Icon from "./Icon.svelte";
  import Ref from "./Ref.svelte";

  const tz = -new Date().getTimezoneOffset();

  let query = $state("");
  let hits = $state<HistoryHit[] | null>(null);
  let searching = $state(false);

  let editing = $state(false);
  let undoable = $state(false);
  let board = $state<Board>();

  // Search as you type, once the typing pauses.
  $effect(() => {
    const q = query.trim();
    if (q.length < 2) {
      hits = null;
      return;
    }
    const timer = setTimeout(async () => {
      searching = true;
      try {
        const found = await api.history(q, tz);
        if (query.trim() === q) hits = found;
      } catch (e) {
        hub.fail(e);
      } finally {
        searching = false;
      }
    }, 250);
    return () => clearTimeout(timer);
  });

  const KIND = { message: "You in Hub", prompt: "Claude Code", commit: "Commit", day: "Devlog" } as const;

  function day(date: string) {
    return new Date(date + "T12:00").toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" });
  }

  /** Shows a day in the Day card, adding the card if it is not on the page. */
  function show(date: string) {
    pickDay("day", date);
    if (!board?.has("day")) board?.addAt("day");
    requestAnimationFrame(() => document.querySelector(".overview [data-card='day']")?.scrollIntoView({ behavior: "smooth", block: "start" }));
  }

  function go(h: HistoryHit) {
    if (h.thread_id && hub.threads.some((t) => t.id === h.thread_id)) hub.open(h.thread_id);
    else if (h.date) {
      query = "";
      show(h.date);
    }
  }

  /** The matched words, lit. */
  function parts(text: string): { t: string; hit: boolean }[] {
    const q = query.trim().toLowerCase();
    const out: { t: string; hit: boolean }[] = [];
    let rest = text;
    while (q && rest) {
      const at = rest.toLowerCase().indexOf(q);
      if (at < 0) break;
      out.push({ t: rest.slice(0, at), hit: false }, { t: rest.slice(at, at + q.length), hit: true });
      rest = rest.slice(at + q.length);
    }
    out.push({ t: rest, hit: false });
    return out;
  }
</script>

<div class="overview">
  <div class="page-fill">
    <div class="top">
      <h1>Overview</h1>
      {#if editing}
        <button class="ghost" onclick={() => board?.back()} disabled={!undoable} data-tip="Undo the last change (Ctrl Z)">Undo</button>
        <button class="ghost" onclick={() => board?.reset()} data-tip="Back to the cards Overview starts with">Reset</button>
      {/if}
      <button class="customize" class:on={editing} onclick={() => (editing = !editing)}>{editing ? "Done" : "Customize"}</button>
    </div>

    <label class="search">
      <Icon name="search" size={14} />
      <input bind:value={query} placeholder="Search history" spellcheck="false" />
      {#if searching}<span class="muted">…</span>{/if}
    </label>

    {#if hits}
      <section>
        <h3>{hits.length ? `${hits.length}${hits.length === 60 ? "+" : ""} found` : "Nothing found"}</h3>
        {#each hits as h}
          <button class="hit" onclick={() => go(h)} disabled={!h.thread_id && !h.date}>
            <span class="when">{h.date ? day(h.date) : ""}</span>
            <span class="body">
              <span class="text">{#each parts(h.text) as p}{#if p.hit}<mark>{p.t}</mark>{:else}{p.t}{/if}{/each}</span>
              <span class="meta">
                {KIND[h.kind]}
                {#if h.kind === "commit" && h.title}<span class="sep">·</span><Ref kind="repo" label={h.title} title={h.cwd ?? ""} />
                {:else}
                  {#if h.title}<span class="sep">·</span><span class="title">{h.title}</span>{/if}
                  {#if h.cwd}<span class="sep">·</span><Ref kind="project" label={folderName(h.cwd)} title={h.cwd} />{/if}
                {/if}
              </span>
            </span>
          </button>
        {/each}
      </section>
    {:else}
      <Board bind:this={board} page="overview" {editing} bind:undoable>
        {#snippet content(id)}<CardBody {id} {show} />{/snippet}
      </Board>
    {/if}
  </div>
</div>

<style>
  .overview { flex: 1; overflow-y: auto; min-width: 0; }
  h1 { color: var(--text); font-size: var(--title); font-weight: 500; letter-spacing: -0.01em; margin: 0 0 14px; }
  .search { display: flex; align-items: center; gap: 8px; background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 8px 10px; margin-bottom: 10px; color: var(--muted); }
  .search:focus-within { border-color: var(--accent); }
  .search input { flex: 1; background: none; border: 0; outline: none; color: var(--text); font: inherit; }
  section { background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-lg); padding: 16px; }
  .top { display: flex; align-items: center; gap: 8px; }
  .top h1 { flex: 1; }
  .customize, .ghost { font-size: 13px; color: var(--text-2); background: var(--surface-hover); border-radius: var(--r-md); padding: 4px 12px; }
  .ghost { background: none; color: var(--muted); }
  .customize:hover, .ghost:hover { color: var(--text); }
  .customize.on { background: var(--accent); color: var(--bg); }
  .ghost:disabled { opacity: 0.4; cursor: default; }

  h3 { color: var(--text); font-size: 15px; font-weight: 600; margin: 0 0 10px; }
  .muted { color: var(--muted); font-size: 12px; }
  .hit { display: flex; gap: 14px; width: 100%; text-align: left; padding: 8px 8px; margin: 0 -8px; border-radius: var(--r-md); width: calc(100% + 16px); }
  .hit:hover:not(:disabled) { background: var(--surface-hover); }
  .hit:disabled { cursor: default; }
  .when { flex: none; width: 92px; color: var(--muted); font-size: 13px; }
  .body { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .text { color: var(--text-2); }
  .meta { display: flex; align-items: center; gap: 4px; min-width: 0; color: var(--muted); font-size: 12px; white-space: nowrap; }
  .meta :global(.glyph) { width: 11px; height: 11px; margin-right: 4px; }
  .meta .title { overflow: hidden; text-overflow: ellipsis; }
  .sep { opacity: 0.7; }
  mark { background: var(--accent-bg); color: var(--text); border-radius: 3px; padding: 0 1px; }
</style>
