<!-- What right-clicking an Overview card opens, with or without Customize on: everything about the card that is not its content. Its title (off until you switch it on, in your own words), its knobs (period, shape, sources, Check now: what the card used to draw in its own head), how its numbers are counted, its colour, how see-through it is, whether its frame shows, its width, fitting its content, links, splitting a combined card, and taking it off. The shared ContextMenu holds only plain items, so this one is its own small panel.
     Used by Board.svelte. -->
<script lang="ts">
  import { pop } from "../popover";
  // What right-clicking an Overview card opens, with or without Customize on: everything about
  // the card that is not its content. Its title (off until you switch it on, in your own words),
  // its knobs (period, shape, sources, Check now: what the card used to draw in its own head),
  // how its numbers are counted, its colour, how see-through it is, whether its frame shows, its
  // width, fitting its content, links, splitting a combined card, and taking it off.
  // The shared ContextMenu holds only plain items, so this one is its own small panel.
  import { SIZE_COLS, type Size } from "../board";
  import { TINTS, about, act as press, cardOf, frameTitle, knob, lookOf, setFrameTitle, setKnob, setLook, widgetOf, type Share } from "../overview.svelte";
  import { prefs } from "../prefs.svelte";
  import { hub } from "../state.svelte";
  import Switch from "./Switch.svelte";

  let {
    id,
    title,
    x,
    y,
    width,
    fitted,
    close,
    resize,
    fit,
    remove,
    parts = [],
    split,
    partners,
    link,
    share,
    widget = false,
    change,
    howOpen = false,
    how,
  }: {
    id: string;
    title: string;
    /** Where it was opened, in page pixels. */
    x: number;
    y: number;
    /** The card's width now, in columns. */
    width: number;
    /** Whether the card follows what it holds (no chosen height). */
    fitted: boolean;
    close: () => void;
    resize: (w: number) => void;
    fit: () => void;
    remove: () => void;
    /** The cards combined into this one, if any, and what takes them apart again. */
    parts?: string[];
    split?: () => void;
    /** The other cards on the board, what a link with each could share and what it shares now. */
    partners: { id: string; title: string; can: Share[]; shared: Share[] }[];
    /** Links with all it can share, or unlinks. */
    link: (other: string) => void;
    /** Turns one thing a link shares on or off. */
    share: (other: string, what: Share) => void;
    /** A widget an agent wrote: it can be asked for a change. */
    widget?: boolean;
    change?: (text: string) => Promise<void>;
    /** Whether the card's How it's counted text is open, and what opens or closes it. */
    howOpen?: boolean;
    how?: () => void;
  } = $props();

  /** The card and the cards combined into it, each with its knobs and name. */
  const members = $derived([id, ...parts].map((m) => ({ id: m, name: nameOf(m), knobs: cardOf(m)?.knobs ?? [] })));
  const nameOf = (m: string) => cardOf(m)?.title ?? widgetOf(m)?.title ?? "Widget";
  const titled = $derived.by(() => {
    void prefs.overviewFrames;
    return frameTitle(id, title) !== null;
  });
  const titleText = $derived.by(() => {
    void prefs.overviewFrames;
    return prefs.overviewFrames?.[id]?.title ?? "";
  });
  const value = (m: string, k: string) => {
    void prefs.overviewFrames;
    return knob<string | boolean>(m, k);
  };

  let asking = $state("");
  let sending = $state(false);
  async function send() {
    const text = asking.trim();
    if (!text || !change || sending) return;
    sending = true;
    try {
      await change(text);
      close();
    } catch (e) {
      hub.fail(e);
    } finally {
      sending = false;
    }
  }

  let el = $state<HTMLDivElement>();
  let pos = $state({ x: 0, y: 0 });
  const look = $derived(lookOf(id));
  const alpha = $derived(look.alpha ?? 1);

  const SIZES: { id: Size; label: string; tip: string }[] = [
    { id: "s", label: "S", tip: "A third of the width" },
    { id: "m", label: "M", tip: "Half the width" },
    { id: "l", label: "L", tip: "Two thirds of the width" },
    { id: "full", label: "Full", tip: "The whole width" },
  ];

  // Keep it on screen.
  $effect(() => {
    if (!el) return;
    const { width: w, height: h } = el.getBoundingClientRect();
    const zoom = Number(document.documentElement.style.zoom) || 1;
    const vw = window.innerWidth / zoom, vh = window.innerHeight / zoom;
    pos = { x: x + w / zoom > vw - 4 ? Math.max(4, x - w / zoom) : x, y: y + h / zoom > vh - 4 ? Math.max(4, vh - h / zoom - 4) : y };
    el.focus({ preventScroll: true });
  });

  function outside(e: Event) {
    if (el && !e.composedPath().includes(el)) close();
  }
  function act(f: () => void) {
    f();
    close();
  }
</script>

<svelte:window onpointerdowncapture={outside} onwheelcapture={outside} onblur={close} onresize={close} />

<div
  class="card-menu"
  transition:pop|global
  role="menu"
  tabindex="-1"
  aria-label="{title} card"
  bind:this={el}
  style:left="{pos.x || x}px"
  style:top="{pos.y || y}px"
  oncontextmenu={(e) => e.preventDefault()}
  onkeydown={(e) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    } else if (e.key === "Delete") act(remove);
  }}
>
  <div class="head">{title}</div>
  {#if widget}
    <div class="label">Ask AI to change this card</div>
    <div class="ask">
      <textarea
        rows="2"
        placeholder="Bigger numbers, a red line at 17:00…"
        bind:value={asking}
        disabled={sending}
        onkeydown={(e) => {
          if (e.key === "Enter" && !e.shiftKey) {
            e.preventDefault();
            send();
          }
          // Typing keys (Delete too) stay in the box; Escape still closes the menu.
          if (e.key !== "Escape") e.stopPropagation();
        }}
      ></textarea>
      <button class="go" disabled={!asking.trim() || sending} onclick={send}>{sending ? "…" : "Send"}</button>
    </div>
    {#if widgetOf(id)?.thread}
      <button class="item" onclick={() => act(() => hub.open(widgetOf(id)!.thread!))}><span class="tick"></span>Open the thread that made it</button>
    {/if}
    <div class="sep"></div>
  {/if}
  <div class="label row">
    <span>Title</span>
    <span class="grow"></span>
    <Switch on={titled} label="Show a title" onchange={(on) => setFrameTitle(id, on)} />
  </div>
  {#if titled}
    <div class="field">
      <input
        type="text"
        value={titleText}
        placeholder={title}
        spellcheck="false"
        aria-label="The title"
        oninput={(e) => setFrameTitle(id, true, e.currentTarget.value)}
        onkeydown={(e) => {
          if (e.key === "Escape") {
            e.stopPropagation();
            close();
          } else e.stopPropagation();
        }}
      />
    </div>
  {/if}
  {#if members.some((m) => m.knobs.length || about[m.id])}
    <div class="sep"></div>
    <div class="label">Settings</div>
    {#each members as m (m.id)}
      {#if members.length > 1 && (m.knobs.length || about[m.id])}<div class="member">{m.name}</div>{/if}
      {#each m.knobs as k (k.id)}
        {#if k.kind === "choice"}
          <div class="label knob"><span>{k.label}</span></div>
          <div class="sizes">
            {#each k.options as o (o.value)}
              <button class:on={value(m.id, k.id) === o.value} onclick={() => setKnob(m.id, k.id, o.value)}>{o.label}</button>
            {/each}
          </div>
        {:else if k.kind === "switch"}
          <button class="item" role="menuitemcheckbox" aria-checked={value(m.id, k.id) === true} onclick={() => setKnob(m.id, k.id, value(m.id, k.id) !== true)}>
            <span class="tick">{value(m.id, k.id) === true ? "✓" : ""}</span>{k.label}
          </button>
        {:else}
          <button class="item" onclick={() => act(() => press(m.id, k.id))}><span class="tick"></span>{k.label}</button>
        {/if}
      {/each}
      {#if about[m.id] && m.id === id}
        <button class="item" role="menuitemcheckbox" aria-checked={howOpen} onclick={() => act(() => how?.())}><span class="tick">{howOpen ? "✓" : ""}</span>How it's counted</button>
      {/if}
    {/each}
  {/if}
  <div class="sep"></div>
  <div class="label">Colour</div>
  <div class="swatches">
    <button class="swatch none" class:on={!look.tint} onclick={() => setLook(id, { tint: undefined })} data-tip="No colour" aria-label="No colour"></button>
    {#each TINTS as t}
      <button class="swatch" class:on={look.tint === t} style:background="var(--p-{t})" onclick={() => setLook(id, { tint: t })} data-tip={t[0].toUpperCase() + t.slice(1)} aria-label={t}></button>
    {/each}
  </div>
  <label class="label slider">
    <span>See-through</span><span class="value">{Math.round((1 - alpha) * 100)}%</span>
    <input type="range" min="0" max="100" step="5" value={Math.round((1 - alpha) * 100)} oninput={(e) => setLook(id, { alpha: 1 - Number(e.currentTarget.value) / 100 })} />
  </label>
  <button class="item" role="menuitemcheckbox" aria-checked={!!look.bare} onclick={() => setLook(id, { bare: !look.bare })}>
    <span class="tick">{look.bare ? "✓" : ""}</span>Hide the frame
  </button>
  <div class="sep"></div>
  <div class="label">Width</div>
  <div class="sizes">
    {#each SIZES as s}
      <button class:on={width === SIZE_COLS[s.id]} onclick={() => act(() => resize(SIZE_COLS[s.id]))} data-tip={s.tip}>{s.label}</button>
    {/each}
    <button class:on={fitted} disabled={fitted} onclick={() => act(fit)} data-tip="As tall as what it holds">Fit content</button>
  </div>
  {#if partners.length}
    <div class="sep"></div>
    <div class="label">Linked with</div>
    <p class="hint">Linked cards follow each other: the time picked in one, or its colour and look.</p>
    {#each partners as p (p.id)}
      <div class="partner">
        <button class="item" role="menuitemcheckbox" aria-checked={p.shared.length > 0} onclick={() => link(p.id)}>
          <span class="tick">{p.shared.length ? "✓" : ""}</span><span class="name">{p.title}</span>
        </button>
        {#if p.shared.length}
          <span class="shares">
            {#each p.can as s}
              <button
                class:on={p.shared.includes(s)}
                aria-pressed={p.shared.includes(s)}
                onclick={() => share(p.id, s)}
                data-tip={s === "time" ? "A day or range picked in one shows in the other" : "Colour, see-through and frame change on both"}
              >{s === "time" ? "Time" : "Look"}</button>
            {/each}
          </span>
        {/if}
      </div>
    {/each}
  {/if}
  <div class="sep"></div>
  {#if parts.length && split}
    <button class="item" onclick={() => act(split)}><span class="tick"></span>Split into {parts.length + 1} cards</button>
  {/if}
  {#if look.tint || look.bare || alpha !== 1}
    <button class="item" onclick={() => setLook(id, { tint: undefined, alpha: 1, bare: false })}><span class="tick"></span>Plain look again</button>
  {/if}
  <button class="item danger" onclick={() => act(remove)}><span class="tick"></span>Take off the page<span class="key">Del</span></button>
</div>

<style>
  .card-menu { position: fixed; z-index: 60; width: 264px; max-height: calc(100vh - 8px); overflow-y: auto; background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 6px; box-shadow: 0 12px 34px rgba(0, 0, 0, 0.5); font-size: 13px; outline: none; }
  .head { color: var(--text); font-weight: 500; padding: 4px 8px 2px; }
  .label { display: flex; color: var(--muted); font-size: 12px; padding: 6px 8px 3px; }
  .label.row { align-items: center; gap: 8px; }
  .grow { flex: 1; }
  .label.knob { padding-top: 4px; }
  .member { color: var(--text-3); font-size: 12px; padding: 4px 8px 0; }
  .field { padding: 2px 8px 4px; }
  .field input { width: 100%; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-sm); color: var(--text); font: inherit; font-size: 12px; padding: 4px 8px; outline: none; }
  .field input:focus { border-color: var(--accent); }
  .slider { flex-wrap: wrap; gap: 0 6px; }
  .slider .value { margin-left: auto; color: var(--text-3); }
  .slider input { width: 100%; margin: 6px 0 2px; accent-color: var(--accent); }
  .swatches { display: flex; flex-wrap: wrap; gap: 6px; padding: 2px 8px 4px; }
  .swatch { width: 17px; height: 17px; border-radius: 50%; border: 2px solid transparent; box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.25); }
  .swatch.none { background: var(--surface-hover); box-shadow: inset 0 0 0 1px var(--line-strong); }
  .swatch.on { border-color: var(--text); }
  .item { display: flex; align-items: center; width: 100%; text-align: left; padding: 4px 8px; border-radius: var(--r-sm); color: var(--text-2); }
  .item:hover, .item:focus-visible { background: var(--surface-hover); color: var(--text); }
  .item.danger { color: var(--red); }
  .tick { width: 16px; flex: none; color: var(--accent); }
  .key { margin-left: auto; color: var(--muted); font-size: 11px; }
  .sizes { display: flex; flex-wrap: wrap; gap: 3px; padding: 2px 6px 4px; }
  .sizes button { font-size: 12px; padding: 2px 8px; border-radius: var(--r-sm); color: var(--text-3); background: var(--surface-hover); }
  .sizes button:hover:not(:disabled) { color: var(--text); }
  .sizes button.on { background: var(--accent-bg); color: var(--text); }
  .sizes button:disabled { cursor: default; }
  .partner { display: flex; align-items: center; gap: 4px; }
  .partner .item { flex: 1; min-width: 0; }
  .partner .name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .shares { display: flex; gap: 3px; flex: none; padding-right: 4px; }
  .shares button { font-size: 11px; padding: 1px 6px; border-radius: var(--r-sm); color: var(--muted); background: var(--surface-hover); }
  .shares button:hover { color: var(--text); }
  .shares button.on { background: var(--accent-bg); color: var(--text); }
  .hint { color: var(--muted); font-size: 11px; margin: -2px 8px 3px; }
  .ask { display: flex; gap: 6px; align-items: flex-end; padding: 2px 8px 6px; }
  .ask textarea { flex: 1; min-width: 0; resize: none; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-sm); color: var(--text); font: inherit; font-size: 12px; padding: 4px 8px; outline: none; }
  .ask textarea:focus { border-color: var(--accent); }
  .go { font-size: 12px; padding: 3px 8px; border-radius: var(--r-sm); background: var(--accent); color: var(--bg); }
  .go:disabled { opacity: 0.4; }
  .sep { border-top: 1px solid var(--line-strong); margin: 4px 2px; }
</style>
