<!-- Finds a doc by its name, a heading or its text. Empty, it lists the docs changed last; `#tag` keeps docs with that tag. Enter opens the doc at the section that matched.
     Used by DocsView.svelte. -->
<script lang="ts">
  import { untrack } from "svelte";
  import { quietly } from "../quiet";
  import { pop } from "../popover";
  import { api } from "../api";
  import type { DocEntry } from "../bindings/DocEntry";
  import type { SearchHit } from "../bindings/SearchHit";
  import { portal } from "../portal";
  import Icon from "./Icon.svelte";

  // Finds a doc by its name, a heading or its text. Empty, it lists the docs changed last;
  // `#tag` keeps docs with that tag. Enter opens the doc at the section that matched.
  let {
    path,
    host = null,
    query = "",
    docs,
    onpick,
    onclose,
  }: {
    path: string;
    host?: string | null;
    query?: string;
    docs: DocEntry[];
    onpick: (rel: string, heading: string | null) => void;
    onclose: () => void;
  } = $props();

  let q = $state(untrack(() => query));
  let hits = $state<SearchHit[]>([]);
  let selected = $state(0);
  let input = $state<HTMLInputElement>();

  $effect(() => {
    input?.focus();
  });

  type Row = { rel: string; title: string; heading: string | null; text: string };
  const recent = $derived(docs.slice(0, 12).map((d): Row => ({ rel: d.rel, title: d.title, heading: null, text: d.summary })));
  const rows = $derived(
    q.trim() ? hits.map((h): Row => ({ rel: h.rel, title: h.title, heading: h.heading?.text ?? null, text: h.snippet })) : recent,
  );

  let asked = 0;
  $effect(() => {
    const text = q.trim();
    if (!text) return;
    const n = ++asked;
    const timer = setTimeout(() => {
      api
        .vaultSearch(path, text, host)
        .then((h) => {
          if (n !== asked) return;
          hits = h;
          selected = 0;
        })
        .catch(quietly("doc search"));
    }, 80);
    return () => clearTimeout(timer);
  });

  function key(e: KeyboardEvent) {
    if (e.key === "ArrowDown") selected = Math.min(rows.length - 1, selected + 1);
    else if (e.key === "ArrowUp") selected = Math.max(0, selected - 1);
    else if (e.key === "Enter" && rows[selected]) pick(rows[selected]);
    else if (e.key === "Escape") onclose();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }
  const pick = (r: Row) => onpick(r.rel, r.heading);

  // The query's words marked in a line of text.
  const words = $derived(q.trim().toLowerCase().split(/\s+/).filter((w) => w && !w.startsWith("#")));
  function marks(text: string): { t: string; hit: boolean }[] {
    if (!words.length) return [{ t: text, hit: false }];
    const re = new RegExp(`(${words.map((w) => w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("|")})`, "gi");
    return text.split(re).filter(Boolean).map((t) => ({ t, hit: words.includes(t.toLowerCase()) }));
  }
  const plain = (s: string) => s.replace(/\[\[([^\]|#]*)(#[^\]|]*)?\|?([^\]]*)\]\]/g, (_, t, h, a) => a || t || h?.slice(1) || "");
</script>

<div role="presentation" class="backdrop" use:portal onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="box" role="dialog" tabindex="-1" aria-modal="true" aria-label="Find a doc" transition:pop|global={{ origin: "50% 0" }}>
    <div class="in">
      <Icon name="search" size={15} />
      <input bind:this={input} bind:value={q} onkeydown={key} placeholder="Find a doc by name, heading, text or #tag" spellcheck="false" />
    </div>
    {#if !q.trim()}<div class="label">Changed lately</div>{/if}
    <div class="list">
      {#each rows as r, i (r.rel + (r.heading ?? ""))}
        <button class="hit" class:active={i === selected} onmouseenter={() => (selected = i)} onclick={() => pick(r)}>
          <span class="t">
            {#each marks(r.title) as m, k (k)}{#if m.hit}<mark>{m.t}</mark>{:else}{m.t}{/if}{/each}
            {#if r.heading && r.heading !== r.title}<span class="h"> › {r.heading}</span>{/if}
            <span class="rel mono">{r.rel}</span>
          </span>
          {#if r.text}
            <span class="s">{#each marks(plain(r.text)) as m, k (k)}{#if m.hit}<mark>{m.t}</mark>{:else}{m.t}{/if}{/each}</span>
          {/if}
        </button>
      {:else}
        <p class="none">{q.trim() ? "No doc matches." : "No docs yet."}</p>
      {/each}
    </div>
    <div class="foot"><span><kbd>↑↓</kbd> move</span><span><kbd>Enter</kbd> open</span><span><kbd>Esc</kbd> close</span></div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; z-index: 60; background: rgb(0 0 0 / 0.35); display: flex; justify-content: center; align-items: flex-start; padding-top: 12vh; }
  .box { width: min(680px, calc(100vw - 32px)); background: var(--surface); border: 1px solid var(--line-strong); border-radius: var(--r-lg); box-shadow: 0 20px 60px rgb(0 0 0 / 0.5); overflow: hidden; display: flex; flex-direction: column; max-height: 70vh; }
  .in { display: flex; align-items: center; gap: 10px; padding: 12px 16px; border-bottom: 1px solid var(--line); color: var(--muted); }
  .in input { flex: 1; background: none; border: 0; outline: none; color: var(--text); font: 15px var(--font); }
  .label { color: var(--muted); font-size: 12px; margin: 10px 16px 2px; }
  .list { overflow-y: auto; padding: 4px 0 6px; }
  .hit { display: flex; flex-direction: column; align-items: flex-start; gap: 1px; width: 100%; text-align: left; padding: 8px 16px; }
  .hit.active { background: var(--surface-hover); }
  .t { color: var(--text); font-size: 14px; }
  .h { color: var(--text-3); }
  .rel { color: var(--muted); font-size: 11px; margin-left: 8px; }
  .s { color: var(--text-3); font-size: 13px; line-height: 1.5; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 100%; }
  mark { background: var(--accent-bg); color: var(--text); border-radius: 3px; padding: 0 1px; }
  .none { color: var(--muted); font-size: 13px; margin: 10px 16px; }
  .foot { display: flex; gap: 16px; padding: 8px 16px; border-top: 1px solid var(--line); color: var(--muted); font-size: 12px; }
  kbd { font: 11px var(--mono); border: 1px solid var(--line-strong); border-radius: var(--r-xs); padding: 0 4px; }
</style>
