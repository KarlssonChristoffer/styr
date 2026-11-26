<!-- Hub's own dropdown, used instead of the browser's <select> everywhere, so every list looks and behaves the same: dark, dense, the right-click menu's look. A button shows the choice; the list opens under it (or above it near the bottom), outside any scrolling box. Keys: Enter, Space or the arrows open it, arrows move, Enter picks, Escape closes, letters jump to a label.
     Used by ApiConnections.svelte, Customs.svelte, DevMode.svelte, EngineKnobs.svelte and 18 more. -->
<script lang="ts" module>
  /** One choice in a [`Select`]. `group` starts a heading above the first option that has it. */
  export type Option<T> = { value: T; label: string; hint?: string; group?: string; disabled?: boolean; /** A CSS font-family: the label shows in that font (the font pickers). */ font?: string };
</script>

<script lang="ts" generics="T">
  import { pop } from "../popover";
  // Hub's own dropdown, used instead of the browser's <select> everywhere, so every list looks
  // and behaves the same: dark, dense, the right-click menu's look. A button shows the choice;
  // the list opens under it (or above it near the bottom), outside any scrolling box. Keys: Enter,
  // Space or the arrows open it, arrows move, Enter picks, Escape closes, letters jump to a label.
  import { tick, untrack } from "svelte";
  import Icon from "./Icon.svelte";

  let {
    value = $bindable(),
    options,
    onchange,
    placeholder = "Choose…",
    disabled = false,
    size = "normal",
    mono = false,
    id,
    label,
    tip,
    autoopen = false,
    searchable = false,
    custom,
    onclose,
    class: klass = "",
  }: {
    value?: T;
    options: Option<T>[];
    onchange?: (value: T) => void;
    placeholder?: string;
    disabled?: boolean;
    /** "small" for chips and table rows, "bare" for a trigger with no border (inside another field). */
    size?: "normal" | "small" | "bare";
    mono?: boolean;
    id?: string;
    /** The accessible name, when no <label for> names it. */
    label?: string;
    tip?: string;
    /** Opens at once (a "Move to…" that replaces a button). */
    autoopen?: boolean;
    /** A search box tops the list and filters it by label. */
    searchable?: boolean;
    /** With `searchable`: what the typed text can also stand for ("Use “Charter”"), added under the matches. */
    custom?: (typed: string) => Option<T> | null;
    /** Called when the list closes, whether or not something was picked. */
    onclose?: () => void;
    class?: string;
  } = $props();

  let trigger = $state<HTMLButtonElement>();
  let list = $state<HTMLDivElement>();
  let open = $state(false);
  let at = $state(-1);
  let pos = $state({ x: 0, y: 0, w: 0, up: false });
  let typed = "";
  let typedAt = 0;
  let query = $state("");

  /** What the list shows: all of it, or what the search box lets through. */
  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!searchable || !q) return options;
    const hits = options.filter((o) => o.label.toLowerCase().includes(q));
    const extra = custom?.(query.trim());
    return extra && !options.some((o) => o.label.toLowerCase() === q) ? [...hits, extra] : hits;
  });

  const current = $derived(options.find((o) => o.value === value));

  async function show() {
    if (disabled || open || !trigger) return;
    const r = trigger.getBoundingClientRect();
    const zoom = Number(document.documentElement.style.zoom) || 1;
    const h = window.innerHeight / zoom;
    const room = h - r.bottom / zoom;
    pos = { x: r.left / zoom, y: r.bottom / zoom + 4, w: r.width / zoom, up: room < Math.min(280, options.length * 28 + 12) && r.top / zoom > room };
    if (pos.up) pos.y = r.top / zoom - 4;
    at = Math.max(0, options.findIndex((o) => o.value === value));
    query = "";
    open = true;
    await tick();
    scrollTo(at);
    if (searchable) list?.querySelector<HTMLInputElement>("input")?.focus();
  }

  function hide(refocus = true) {
    if (!open) return;
    open = false;
    if (refocus) trigger?.focus({ preventScroll: true });
    onclose?.();
  }

  function pick(o: Option<T>) {
    if (o.disabled) return;
    const changed = o.value !== value;
    value = o.value;
    hide();
    if (changed) onchange?.(o.value);
  }

  function scrollTo(i: number) {
    list?.querySelector<HTMLElement>(`[data-i="${i}"]`)?.scrollIntoView({ block: "nearest" });
  }

  function move(step: number) {
    if (!shown.length) return;
    let i = at;
    for (let n = 0; n < shown.length; n++) {
      i = (i + step + shown.length) % shown.length;
      if (!shown[i].disabled) break;
    }
    at = i;
    scrollTo(i);
  }

  function jump(ch: string) {
    const now = Date.now();
    typed = now - typedAt < 700 ? typed + ch.toLowerCase() : ch.toLowerCase();
    typedAt = now;
    const from = typed.length > 1 ? at : at + 1;
    for (let n = 0; n < shown.length; n++) {
      const i = (from + n) % shown.length;
      if (!shown[i].disabled && shown[i].label.toLowerCase().startsWith(typed)) {
        if (open) {
          at = i;
          scrollTo(i);
        } else pick(shown[i]);
        return;
      }
    }
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === "Tab") return hide(false);
    // In the search box, letters and Space are the text.
    if (searchable && open && e.target instanceof HTMLInputElement && (e.key.length === 1 || e.key === "Backspace") && !e.ctrlKey && !e.metaKey) return;
    if (!open) {
      if (["Enter", " ", "ArrowDown", "ArrowUp"].includes(e.key)) {
        e.preventDefault();
        show();
      } else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) jump(e.key);
      return;
    }
    if (e.key === "ArrowDown") move(1);
    else if (e.key === "ArrowUp") move(-1);
    else if (e.key === "Home") (at = -1), move(1);
    else if (e.key === "End") (at = 0), move(-1);
    else if (e.key === "Enter" || e.key === " ") shown[at] && pick(shown[at]);
    else if (e.key === "Escape") hide();
    else if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) jump(e.key);
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  function outside(e: Event) {
    if (open && !e.composedPath().some((n) => n === list || n === trigger)) hide(false);
  }

  $effect(() => {
    // Untracked: show() reads `open`, so closing the list would otherwise open it again.
    if (autoopen && trigger) {
      trigger.focus();
      untrack(show);
    }
  });
</script>

<svelte:window onpointerdowncapture={outside} onwheelcapture={outside} onresize={() => hide(false)} onblur={() => hide(false)} />

<button
  type="button"
  class="select {size} {klass}"
  class:mono
  class:open
  class:empty={!current}
  {id}
  {disabled}
  bind:this={trigger}
  aria-haspopup="listbox"
  aria-expanded={open}
  aria-label={label}
  data-tip={tip}
  onclick={() => (open ? hide() : show())}
  onkeydown={keydown}
>
  <span class="value" style:font-family={current?.font}>{current?.label ?? placeholder}</span>
  <Icon name="chevron" size={11} />
</button>

{#if open}
  <div
    class="list"
    transition:pop|global
    class:above={pos.up}
    class:mono
    role="listbox"
    tabindex="-1"
    bind:this={list}
    style:left="{pos.x}px"
    style:top="{pos.y}px"
    style:min-width="{pos.w}px"
    onkeydown={keydown}
  >
    {#if searchable}<input class="find" bind:value={query} oninput={() => (at = 0)} placeholder="Search…" spellcheck="false" autocomplete="off" />{/if}
    {#each shown as o, i (i)}
      {#if o.group && o.group !== shown[i - 1]?.group}<div class="group">{o.group}</div>{/if}
      <button
        type="button"
        role="option"
        tabindex="-1"
        data-i={i}
        aria-selected={o.value === value}
        class:at={i === at}
        class:chosen={o.value === value}
        disabled={o.disabled}
        onpointermove={() => (at = i)}
        onclick={() => pick(o)}
      >
        <span class="check">{#if o.value === value}<Icon name="check" size={11} />{/if}</span>
        <span class="label" style:font-family={o.font}>{o.label}</span>
        {#if o.hint}<span class="hint">{o.hint}</span>{/if}
      </button>
    {/each}
    {#if searchable && !shown.length}<div class="none">No match</div>{/if}
  </div>
{/if}

<style>
  .select {
    display: inline-flex; align-items: center; gap: 8px; height: 28px; padding: 0 8px 0 8px; max-width: 100%; min-width: 0;
    background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md);
    color: var(--text-2); font-size: 13px; text-align: left; cursor: pointer; outline: none;
  }
  .select:hover:not(:disabled) { border-color: var(--muted); color: var(--text); }
  .select:focus-visible, .select.open { border-color: var(--accent); }
  .select:disabled { opacity: 0.55; cursor: default; }
  .select.small { height: 22px; font-size: 12px; padding: 0 6px 0 8px; gap: 6px; }
  .select.bare { border-color: transparent; background: none; }
  .select.bare:hover:not(:disabled) { border-color: transparent; background: var(--surface-hover); }
  .select.empty .value { color: var(--muted); }
  .select :global(svg) { transform: rotate(90deg); color: var(--muted); flex: none; transition: transform 0.12s var(--ease); }
  .select.open :global(svg) { transform: rotate(-90deg); }
  .value { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mono, .mono .label { font-family: var(--mono); }

  .list {
    position: fixed; z-index: 120; max-width: 420px; max-height: min(340px, 60vh); overflow-y: auto;
    background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 4px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.4); display: flex; flex-direction: column; outline: none;
  }
  .list.above { transform: translateY(-100%); }
  .group { color: var(--muted); font-size: 12px; padding: 8px 10px 3px; }
  .list button {
    display: flex; align-items: center; gap: 8px; text-align: left; padding: 4px 10px 4px 6px; border-radius: var(--r-sm);
    font-size: 13px; color: var(--text-2); outline: none; white-space: nowrap;
  }
  .list button.at { background: var(--surface-hover); color: var(--text); }
  .list button.chosen { color: var(--text); }
  .list button:disabled { color: var(--muted); background: none; }
  .find { position: sticky; top: -4px; z-index: 1; height: 28px; margin: 0 0 4px; padding: 0 8px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-sm); font-size: 13px; outline: none; flex: none; }
  .find:focus { border-color: var(--accent); }
  .none { color: var(--muted); font-size: 12px; padding: 6px 10px; }
  .check { width: 12px; flex: none; display: grid; place-items: center; color: var(--accent); }
  .label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
  .hint { color: var(--muted); font-size: 12px; margin-left: 12px; }
</style>
