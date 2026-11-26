<!-- The model picker the message box opens: a rail of providers (and Favorites), a search, and the models of the tab. It keeps its own tab, search and cursor and starts afresh each time it opens; the box decides what a pick does, since that may start another session.
     Used by Composer.svelte. -->
<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { pop } from "../popover";
  import { prefs } from "../prefs.svelte";
  import { BRAND_RGB } from "../cardSwitch";
  import { modelLabel, type ConfigOption } from "../session";
  import { engineProvider, favoriteKey, favorites, providerOf, toggleFavorite, type tabsOf } from "../providers.svelte";
  import { modelRows, olderCount, SWITCH_WARNING, type ModelRow } from "../modelRows";
  import Icon from "./Icon.svelte";
  import IconSwap from "./IconSwap.svelte";
  import ProviderMark from "./ProviderMark.svelte";

  // The model picker the message box opens: a rail of providers (and Favorites), a search, and the
  // models of the tab. It keeps its own tab, search and cursor and starts afresh each time it opens;
  // the box decides what a pick does, since that may start another session.
  let {
    at,
    engine,
    model,
    engineNames,
    modelsOf,
    tabs,
    tabEngines,
    viaOf,
    unready,
    unreadyWhy,
    canSwitch,
    switching,
    onpick,
    onlocked,
    onfix,
    onmanage,
    onclose,
  }: {
    /** The tab it opens on: a provider's key, an engine, or "favorites". */
    at: string;
    engine: string;
    model: ConfigOption | undefined;
    engineNames: string[];
    modelsOf: (engine: string) => ConfigOption | undefined;
    tabs: ReturnType<typeof tabsOf>;
    tabEngines: (engine: string) => string[];
    viaOf: (engine: string) => string;
    unready: Record<string, { short: string; action?: string | null }>;
    unreadyWhy: (engine: string) => string;
    canSwitch: boolean;
    /** A thread that can switch engines: picking another provider starts a new session. */
    switching: boolean;
    onpick: (row: ModelRow) => void;
    /** A pick that would change provider while a turn runs. */
    onlocked: () => void;
    onfix: (engine: string) => void;
    onmanage: () => void;
    /** Closes the picker and puts the cursor back in the box. */
    onclose: () => void;
  } = $props();

  let rail = $state(untrack(() => at));
  let query = $state("");
  let cursor = $state(0);
  let showOld = $state(false);
  // Search hides behind the magnifier until it is wanted; typing a letter in the picker opens it.
  let searching = $state(false);
  let search = $state<HTMLInputElement>();
  let picker_ = $state<HTMLDivElement>();

  // The picker takes the colour of the provider tab you are on; Favorites and search show every
  // provider, so they keep the colour of the one the thread is on.
  const brand = $derived(prefs.pickerTheme === "off" ? undefined : BRAND_RGB[!query && rail && rail !== "favorites" ? rail : engine]);
  const from = $derived({ query, rail, showOld, engineNames, modelsOf, tabEngines });
  const rows = $derived(modelRows(from));
  const older = $derived(olderCount(from));
  const isCurrent = (r: ModelRow) => r.engine === engine && (!r.choice || (!!model && r.choice.value === model.value));

  onMount(() => picker_?.focus());
  $effect(() => {
    void query;
    void rail;
    cursor = 0;
  });

  function openSearch(first = "") {
    searching = true;
    query += first;
    queueMicrotask(() => search?.focus());
  }
  function closeSearch() {
    searching = false;
    query = "";
    picker_?.focus();
  }
  function pickerKey(e: KeyboardEvent) {
    const n = rows.length;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (n) cursor = (cursor + (e.key === "ArrowDown" ? 1 : n - 1)) % n;
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (rows[cursor]) onpick(rows[cursor]);
    } else if (e.key === "Tab") {
      // Tab walks the rail.
      e.preventDefault();
      const stops = ["favorites", ...tabs.map((t) => t.key)];
      const i = stops.indexOf(rail);
      rail = stops[(i + (e.shiftKey ? stops.length - 1 : 1)) % stops.length];
      query = "";
      searching = false;
      cursor = 0;
    } else if (e.key === "Escape") {
      e.preventDefault();
      if (searching) return closeSearch();
      onclose();
    } else if (!searching && e.key.length === 1 && /\S/.test(e.key) && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      openSearch(e.key);
    }
  }
  /** Ctrl + a digit: picks that row. False when there is none. */
  export function pickNth(n: number): boolean {
    const row = rows[n - 1];
    if (!row) return false;
    onpick(row);
    return true;
  }
</script>

<div
  class="menu picker"
  transition:pop|global
  role="dialog"
  tabindex="-1"
  aria-label="Models"
  data-pt={brand ? prefs.pickerTheme : null}
  style:--b={brand?.a}
  style:--b2={brand?.b}
  bind:this={picker_}
  onkeydown={pickerKey}
>
  {#if brand}<div class="fx" aria-hidden="true"></div>{/if}
  {#if searching}
    <div class="search">
      <input bind:this={search} bind:value={query} placeholder="Search all models…" spellcheck="false" />
      <button class="clear" data-tip="Close search (Esc)" onclick={closeSearch}><Icon name="x" size={12} /></button>
    </div>
  {/if}
  <div class="cols">
    <div class="rail" role="tablist">
      <button class:on={searching} data-tip="Search all models (or just type)" onclick={() => (searching ? closeSearch() : openSearch())}>
        <Icon name="search" size={15} />
      </button>
      <button role="tab" class:on={!query && rail === "favorites"} data-tip="Favorites" onclick={() => { rail = "favorites"; closeSearch(); }}>
        <span class="fav"><Icon name="star" size={15} /></span>
      </button>
      {#each tabs as t (t.key)}
        {@const p = engineProvider(t.key)}
        <button role="tab" class:on={!query && t.engines.includes(rail)} class:current={t.engines.includes(engine)} data-tip="{p.name} · {t.engines.join(', ')}" onclick={() => { rail = t.key; showOld = false; closeSearch(); }}>
          <ProviderMark id={p.id} size={16} />
        </button>
      {/each}
      <span class="grow"></span>
      <button class="add" data-tip="Add or change engines" onclick={onmanage}><Icon name="plus" size={14} /></button>
    </div>
    <div class="list" role="listbox" tabindex="-1" onpointerdown={(e) => searching && !query.trim() && !(e.target as Element).closest(".option") && closeSearch()}>
      {#each rows as r, i (`${r.engine}:${r.choice?.value ?? ""}`)}
        {@const p = providerOf(r.engine, r.choice ? `${r.choice.group ?? ""} ${r.choice.name}` : undefined)}
        {#if r.head}
          <div class="group-head">{r.head}</div>
        {/if}
        {@const fav = !!r.choice && favorites.keys.includes(favoriteKey(r.engine, r.choice))}
        <div
          class="option row model"
          data-tip={r.engine !== engine && unready[r.engine] ? unreadyWhy(r.engine) : r.choice ? [r.choice.value, r.choice.description].filter(Boolean).join(" · ") : undefined}
          class:active={i === cursor}
          class:current={isCurrent(r)}
          class:locked={r.engine !== engine && !canSwitch}
          class:unready={r.engine !== engine && !!unready[r.engine]}
          role="option"
          tabindex="-1"
          aria-selected={isCurrent(r)}
          onpointerenter={() => (cursor = i)}
          onclick={() => (r.engine === engine || canSwitch ? onpick(r) : onlocked())}
          onkeydown={() => {}}
        >
          {#if query || rail === "favorites"}<ProviderMark id={p.id} size={14} />{/if}
          {#if r.choice}
            <span class="grow name">{modelLabel(r.choice)}{#if (query || rail === "favorites") && viaOf(r.engine)}<span class="via">{viaOf(r.engine)}</span>{/if}</span>
          {:else}
            <span class="grow lines">
              <span class="line1">Start with {p.name}</span>
              <span class="line2">{r.engine === engine ? "Its models show once it has started" : "Models show after its first start"}</span>
            </span>
          {/if}
          {#if r.engine !== engine && unready[r.engine]}
            {@const u = unready[r.engine]}
            <span class="why">{u.short}</span>
            {#if u.action}
              <button class="fix" onclick={(ev) => { ev.stopPropagation(); onfix(r.engine); }}>{u.action}</button>
            {/if}
          {/if}
          {#if r.choice}
            {@const choice = r.choice}
            <button class="star" class:on={fav} data-tip={fav ? "Remove from favorites" : "Add to favorites"} onclick={(ev) => { ev.stopPropagation(); toggleFavorite(r.engine, choice); }}>
              <Icon name="star" size={13} />
            </button>
          {/if}
          {#if isCurrent(r)}<span class="tick"><Icon name="check" size={14} /></span>{:else if i < 9}<span class="key">{i + 1}</span>{/if}
        </div>
      {:else}
        <p class="blurb">{query ? "No model matches." : "Star models to keep them here."}</p>
      {/each}
      {#if older}
        <button class="option row older" onclick={() => (showOld = !showOld)}>
          <span class="grow">{showOld ? "Hide more models" : `More models (${older})`}</span><IconSwap name={showOld ? "down" : "chevron"} size={13} />
        </button>
      {/if}
    </div>
  </div>
  {#if switching && rows.some((r) => r.engine !== engine)}
    <p class="foot">{#if canSwitch}<span class="warn">New session.</span> {SWITCH_WARNING}{:else}Let the turn finish before switching.{/if}</p>
  {/if}
</div>

<style>
  .menu {
    position: absolute; z-index: 20; bottom: calc(100% + 6px);
    background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-lg);
    padding: 4px; box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
    max-height: 340px; overflow-y: auto;
  }
  .menu { left: 0; min-width: 240px; max-width: 360px; }
  .option {
    display: flex; align-items: flex-start; gap: 8px; width: 100%; text-align: left;
    padding: 6px 8px; border-radius: var(--r-md); color: var(--text-2);
  }
  .option:hover, .option.active { background: var(--surface-hover); color: var(--text); }
  .option.row { align-items: center; gap: 10px; padding: 4px 10px; font-size: 14px; white-space: nowrap; }
  .option.row .grow { display: inline-flex; align-items: center; gap: 8px; }
  .tick { color: var(--accent); display: grid; }
  .key { color: var(--muted); font-size: 12px; font-variant-numeric: tabular-nums; width: 14px; text-align: center; }
  .blurb { color: var(--muted); font-size: 12px; line-height: 1.45; margin: 4px 10px 6px; white-space: normal; }
  .lines { display: flex !important; flex-direction: column; align-items: flex-start !important; gap: 1px !important; min-width: 0; }
  .line1 { display: inline-flex; align-items: baseline; gap: 8px; color: var(--text); font-size: 14px; }
  .line2 { color: var(--muted); font-size: 12px; white-space: normal; line-height: 1.35; }
  .option.unready .lines { opacity: 0.55; }
  .grow { flex: 1; }
  .via { color: var(--muted); font-size: 0.92em; margin-left: 6px; }
  .via::before { content: "· "; }
  .group-head { padding: 8px 10px 3px; font-size: 12px; color: var(--muted); }
  .warn { color: var(--amber); }
  .foot { color: var(--muted); font-size: 12px; margin: 4px 10px 4px; line-height: 1.4; }
  .menu.picker { width: 260px; max-width: calc(100vw - 32px); max-height: none; padding: 0; overflow: hidden; display: flex; flex-direction: column; }
  .search { display: flex; align-items: center; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--line-strong); color: var(--muted); }
  .search input { flex: 1; min-width: 0; background: none; border: 0; outline: none; font-size: 13px; color: var(--text); }
  .search .clear { color: var(--muted); display: grid; place-items: center; width: 20px; height: 20px; border-radius: var(--r-sm); }
  .search .clear:hover { color: var(--text); background: var(--surface-hover); }
  .menu.picker:focus { outline: none; }
  .menu.picker { isolation: isolate; }
  .menu.picker > :not(.fx) { position: relative; z-index: 1; }
  /* The company's colour behind the picker (Settings > Appearance). It is only a layer behind, so
     nothing moves: the stars and the tick keep their columns. */
  .fx { position: absolute !important; inset: 0; z-index: 0 !important; pointer-events: none; transition: opacity var(--dur-move) var(--ease); }
  .picker[data-pt] .rail button.on { background: rgba(var(--b), 0.14); box-shadow: inset 0 0 0 1px rgba(var(--b), 0.38); }
  .picker[data-pt] .option.model.current .tick { color: rgb(var(--b)); }
  .picker[data-pt="edge"] .fx { background: linear-gradient(180deg, rgba(var(--b), 0.09), transparent 55%); }
  .picker[data-pt="edge"] .fx::before { content: ""; position: absolute; left: 0; right: 0; top: 0; height: 1px; background: linear-gradient(90deg, transparent, rgba(var(--b), 0.9), rgba(var(--b2), 0.6), transparent 80%); }
  .picker[data-pt="edge"] .rail button.on, .picker[data-pt="quiet"] .rail button.on { background: rgba(var(--b), 0.12); box-shadow: none; }
  .picker[data-pt="edge"] .rail button.on::after { content: ""; position: absolute; left: -4px; top: 7px; bottom: 7px; width: 2px; border-radius: 1px; background: rgb(var(--b)); }
  .picker[data-pt="edge"] .option.model.current .name, .picker[data-pt="quiet"] .option.model.current .name { color: rgb(var(--b)); }
  .picker[data-pt="glow"] .fx::before, .picker[data-pt="glow"] .fx::after { content: ""; position: absolute; width: 170px; height: 150px; border-radius: 50%; filter: blur(38px); }
  .picker[data-pt="glow"] .fx::before { background: rgba(var(--b), 0.26); left: -50px; bottom: -40px; }
  .picker[data-pt="glow"] .fx::after { background: rgba(var(--b2), 0.13); right: -60px; top: -60px; }
  .picker[data-pt="glow"] .option.model.current .name { color: var(--text); }
  .picker[data-pt="quiet"] .option.model.active { background: rgba(var(--b), 0.08); }
  .cols { display: flex; min-height: 0; max-height: 280px; }
  .rail { display: flex; flex-direction: column; align-items: center; gap: 1px; padding: 4px 3px; border-right: 1px solid var(--line-strong); }
  .rail button { width: 28px; height: 28px; display: grid; place-items: center; border-radius: var(--r-md); position: relative; }
  .rail button:hover { background: var(--surface-hover); }
  .rail button.on { background: var(--surface-hover); box-shadow: inset 0 0 0 1px var(--line-strong); }
  .rail button.current::after { content: ""; position: absolute; left: -4px; top: 9px; bottom: 9px; width: 2px; border-radius: 1px; background: var(--accent); }
  .rail .fav { color: var(--amber); display: grid; }
  .rail .fav :global(path) { fill: currentColor; }
  .rail .add { color: var(--muted); }
  .list { flex: 1; min-width: 0; overflow-y: auto; padding: 3px; }
  .option.model { cursor: pointer; padding: 3px 8px; font-size: 13px; min-height: 26px; align-items: center; }
  .option.model .name { overflow: hidden; text-overflow: ellipsis; color: var(--text-2); }
  .option.model.active .name { color: var(--text); }
  .option.model.current .name { color: var(--accent); }
  .option.model.locked { opacity: 0.5; cursor: default; }
  .option.model.unready .name { color: var(--text-3); }
  .option.model .why { font-size: 12px; color: var(--amber); white-space: nowrap; }
  .option.model .fix { font-size: 12px; padding: 1px 8px; border-radius: var(--r-sm); color: var(--text); background: var(--surface-hover); white-space: nowrap; flex: none; }
  .option.model .fix:hover { background: var(--line-strong); }
  .option.model .key { width: 14px; font-size: 11px; opacity: 0; }
  .option.model.active .key { opacity: 0.6; }
  .star { color: var(--muted); display: grid; place-items: center; width: 20px; height: 20px; border-radius: var(--r-sm); opacity: 0; flex: none; }
  .option.model:hover .star, .option.model.active .star, .star.on { opacity: 1; }
  .star:hover { color: var(--text); background: var(--line-strong); }
  .star.on { color: var(--amber); }
  .star.on :global(path) { fill: currentColor; }
  .option.older { color: var(--muted); font-size: 12px; padding: 4px 8px; }
  .picker .foot { border-top: 1px solid var(--line-strong); margin: 0; padding: 4px 10px; }
</style>
