<!-- The Add card panel beside the Overview board while customizing: every card with a small picture of what it shows, found by search or by group. Click one to put it on the board (where it fits first) or take it off; drag one onto the board to put it exactly there. On top, Make a widget: say or type what you want, and an agent writes it as a card (`widgets.rs`).
     Used by Board.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // The Add card panel beside the Overview board while customizing: every card with a small
  // picture of what it shows, found by search or by group. Click one to put it on the board
  // (where it fits first) or take it off; drag one onto the board to put it exactly there. On top,
  // Make a widget: say or type what you want, and an agent writes it as a card (`widgets.rs`).
  import Working from "./Working.svelte";
  import { CARDS, widgets, type Group } from "../overview.svelte";
  import { hub } from "../state.svelte";
  import { chirp, Recorder, voiceApi } from "../voice";
  import Icon from "./Icon.svelte";

  let {
    on,
    toggle,
    pick,
    letGo,
    make,
  }: {
    /** The cards on the board now. */
    on: Set<string>;
    toggle: (id: string) => void;
    /** A card is being dragged out of the panel. */
    pick: (id: string) => void;
    letGo: () => void;
    /** Starts a widget from what the user asked for and puts it on the board. */
    make: (text: string) => Promise<void>;
  } = $props();

  type Tab = Group | "All" | "Yours";
  let query = $state("");
  let group = $state<Tab>("All");
  const groups = $derived<Tab[]>(["All", ...new Set(CARDS.map((c) => c.group)), ...(widgets.list.length ? (["Yours"] as const) : [])]);

  const matches = (title: string, blurb: string) => {
    const q = query.trim().toLowerCase();
    return !q || title.toLowerCase().includes(q) || blurb.toLowerCase().includes(q);
  };
  const shown = $derived(CARDS.filter((c) => (group === "All" || c.group === group) && matches(c.title, c.blurb)));
  const yours = $derived(group === "All" || group === "Yours" ? widgets.list.filter((w) => matches(w.title, w.ask)) : []);

  // ---- Make a widget: typed or said; the mic records until pressed again (as in Ctrl I). ----
  let wish = $state("");
  let making = $state(false);
  let box = $state<HTMLTextAreaElement>();
  async function build() {
    const text = wish.trim();
    if (!text || making) return;
    making = true;
    try {
      await make(text);
      wish = "";
    } catch (e) {
      hub.fail(e);
    } finally {
      making = false;
    }
  }
  const recorder = new Recorder();
  let voice = $state<"idle" | "recording" | "transcribing">("idle");
  async function toggleVoice() {
    if (voice === "transcribing") return;
    if (voice === "idle") {
      voiceApi.prepare();
      try {
        await recorder.start(localStorage.getItem("hub.voice.mic") ?? "");
        chirp(true);
        voice = "recording";
      } catch (e) {
        hub.fail(e);
      }
      return;
    }
    const audio = recorder.slice(0);
    await recorder.stop();
    chirp(false);
    voice = "transcribing";
    try {
      const said = await voiceApi.transcribe(audio, wish, true);
      if (said.trim()) wish = wish.trim() ? `${wish.trim()} ${said.trim()}` : said.trim();
    } catch (e) {
      hub.fail(e);
    } finally {
      voice = "idle";
      box?.focus();
    }
  }
  $effect(() => () => {
    if (recorder.active) void recorder.stop();
  });

  // A fixed pattern per card for its little picture, so the shelf reads at a glance.
  const lit = (n: number, seed: number) => Array.from({ length: n }, (_, i) => ((i * 7 + seed * 13) % 11) / 11);
</script>

<aside class="shelf">
  <div class="make">
    <div class="make-head"><Icon name="spark" size={13} />Make a widget</div>
    <div class="make-box">
      <textarea
        bind:this={box}
        bind:value={wish}
        rows="3"
        disabled={making}
        placeholder={voice === "recording" ? "Listening… press the mic again when done" : "Anything you want on the board: a countdown to Friday 17:00, my open threads as dots, tokens per weekday…"}
        onkeydown={(e) => {
          if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
            e.preventDefault();
            build();
          }
        }}
      ></textarea>
    </div>
    <div class="make-bar">
      <button class="mic" class:on={voice === "recording"} disabled={voice === "transcribing"} onclick={toggleVoice} data-tip={voice === "recording" ? "Stop and write it out" : "Say it"} aria-label="Say what the widget should show"><Icon name="mic" size={14} /></button>
      <span class="muted small">{voice === "transcribing" ? "Writing out what you said…" : "An agent builds it in the background on your subscription"}</span>
      <button class="build" disabled={!wish.trim() || making} onclick={build}>{making ? "Starting…" : "Build"}</button>
    </div>
  </div>

  <h3>Add cards</h3>
  <p class="muted">Click to add or remove. Drag to place.</p>
  <label class="find">
    <Icon name="search" size={13} />
    <input bind:value={query} placeholder="Search cards" spellcheck="false" />
  </label>
  <div class="groups">
    {#each groups as g}
      <button class:on={group === g} onclick={() => (group = g)}>{g === "Projects and models" ? "Projects" : g}</button>
    {/each}
  </div>
  {#if yours.length}
    <h4>Yours</h4>
    {#each yours as w (w.id)}
      <div class="tile-row">
        <button
          class="tile"
          class:on={on.has(w.id)}
          class:building={w.state === "building"}
          draggable="true"
          onclick={() => toggle(w.id)}
          ondragstart={(e) => {
            e.dataTransfer?.setData("application/x-hub-card", w.id);
            if (e.dataTransfer) e.dataTransfer.effectAllowed = "copy";
            pick(w.id);
          }}
          ondragend={letGo}
          data-tip={w.ask}
        >
          <span class="thumb" aria-hidden="true">{#if w.state === "building"}<Working />{:else if w.state === "failed"}<Icon name="alert" size={14} />{:else}<Icon name="spark" size={14} />{/if}</span>
          <span class="what"><span class="title">{w.title}</span><span class="blurb">{w.state === "building" ? "Building…" : w.state === "failed" ? "Could not be made" : w.ask}</span></span>
          <span class="mark">{#if on.has(w.id)}<Icon name="check" size={13} />{:else}+{/if}</span>
        </button>
        {#if w.thread}<button class="thread" onclick={() => hub.open(w.thread!)} data-tip="Open the thread that makes it"><Icon name="external" size={12} /></button>{/if}
      </div>
    {/each}
    {#if group === "All" && shown.length}<h4>Built in</h4>{/if}
  {/if}
  {#each shown as card (card.id)}
    <button
      class="tile"
      class:on={on.has(card.id)}
      draggable="true"
      onclick={() => toggle(card.id)}
      ondragstart={(e) => {
        e.dataTransfer?.setData("application/x-hub-card", card.id);
        if (e.dataTransfer) e.dataTransfer.effectAllowed = "copy";
        pick(card.id);
      }}
      ondragend={letGo}
      data-tip={on.has(card.id) ? "On the board · click to take it off" : "Click to add, or drag onto the board"}
    >
      <span class="thumb" aria-hidden="true">
        {#if card.id === "map"}
          <span class="cells wide">{#each lit(36, 1) as v}<i style:opacity={0.25 + v * 0.75}></i>{/each}</span>
        {:else if card.id === "clock"}
          <span class="cells">{#each lit(24, 3) as v}<i style:opacity={0.2 + v * 0.8}></i>{/each}</span>
        {:else if card.id === "lines"}
          <span class="bars">{#each lit(10, 2) as v}<i style:height="{20 + v * 70}%"></i>{/each}</span>
        {:else if card.id === "stats"}
          <span class="rows">{#each [90, 40, 22, 12] as w}<i style:width="{w}%"></i>{/each}</span>
        {:else if card.id === "health"}
          <span class="dots">{#each ["red", "red", "amber", "green"] as c}<i class={c}></i>{/each}</span>
        {:else if card.id === "customs"}
          <span class="dots">{#each ["green", "green", "amber", "green"] as c}<i class={c}></i>{/each}</span>
        {:else if card.id === "records"}
          <span class="badges"><i></i><i></i><i></i><i></i></span>
        {:else}
          <span class="lines">{#each [80, 60, 70, 40] as w}<i style:width="{w}%"></i>{/each}</span>
        {/if}
      </span>
      <span class="what"><span class="title">{card.title}</span><span class="blurb">{card.blurb}</span></span>
      <span class="mark">{#if on.has(card.id)}<Icon name="check" size={13} />{:else}+{/if}</span>
    </button>
  {:else}
    <Quiet text="No card matches" />
  {/each}
</aside>

<style>
  .make { border: 1px dashed var(--line-strong); border-radius: var(--r-md); padding: 10px; margin-bottom: 14px; }
  .make-head { display: flex; align-items: center; gap: 6px; color: var(--text); font-size: 13px; margin-bottom: 6px; }
  .make-box textarea { width: 100%; resize: vertical; background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-md); color: var(--text); font: inherit; font-size: 13px; padding: 8px 8px; outline: none; box-sizing: border-box; }
  .make-box textarea:focus { border-color: var(--accent); }
  .make-bar { display: flex; align-items: center; gap: 8px; margin-top: 6px; }
  .make-bar .small { flex: 1; margin: 0; font-size: 11px; }
  .mic { flex: none; color: var(--text-3); padding: 4px; border-radius: var(--r-sm); }
  .mic:hover { color: var(--text); background: var(--surface-hover); }
  .mic.on { color: var(--red); box-shadow: inset 0 0 0 1px var(--red); }
  .build { flex: none; font-size: 12px; padding: 3px 11px; border-radius: var(--r-sm); background: var(--accent); color: var(--bg); }
  .build:disabled { opacity: 0.4; cursor: default; }
  h4 { color: var(--muted); font-size: 11px; font-weight: 500; margin: 8px 0 2px; }
  .tile-row { display: flex; align-items: center; }
  .tile-row .tile { flex: 1; min-width: 0; }
  .thread { flex: none; color: var(--muted); padding: 4px; border-radius: var(--r-sm); }
  .thread:hover { color: var(--text); background: var(--surface-hover); }
  .tile.building { opacity: 0.6; }
  .tile.building .thumb { background: repeating-linear-gradient(135deg, var(--bg) 0 6px, var(--surface-2) 6px 12px); }
  .shelf { flex: none; width: var(--panel-w); position: sticky; top: 0; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-lg); padding: 14px; max-height: calc(100vh - 120px); overflow-y: auto; }
  h3 { color: var(--text); font-size: 15px; font-weight: 600; margin: 0 0 4px; }
  .muted { color: var(--muted); font-size: 12px; margin: 0 0 10px; }
  .find { display: flex; align-items: center; gap: 8px; background: var(--surface-2); border: 1px solid var(--line-strong); border-radius: var(--r-md); padding: 4px 8px; color: var(--muted); }
  .find:focus-within { border-color: var(--accent); }
  .find input { flex: 1; min-width: 0; background: none; border: 0; outline: none; color: var(--text); font: inherit; font-size: 13px; }
  .groups { display: flex; flex-wrap: wrap; gap: 3px; margin: 8px 0 8px; }
  .groups button { font-size: 12px; padding: 2px 8px; border-radius: var(--r-sm); color: var(--muted); }
  .groups button:hover { color: var(--text); }
  .groups button.on { background: var(--surface-hover); color: var(--text); }
  .tile { display: flex; gap: 10px; align-items: center; width: 100%; text-align: left; padding: 6px; border-radius: var(--r-md); cursor: grab; }
  .tile:hover { background: var(--surface-hover); }
  .thumb { flex: none; width: 64px; height: 42px; border-radius: var(--r-sm); background: var(--bg); border: 1px solid var(--line-strong); display: grid; place-items: center; overflow: hidden; padding: 4px; }
  .tile.on .thumb { border-color: var(--accent); }
  .thumb i { display: block; background: var(--accent); border-radius: 1.5px; }
  .cells { display: grid; grid-template-columns: repeat(6, 5px); gap: 2px; }
  .cells.wide { grid-template-columns: repeat(12, 3px); gap: 1.5px; }
  .cells i { aspect-ratio: 1; }
  .bars { display: flex; align-items: flex-end; gap: 2px; width: 100%; height: 100%; }
  .bars i { flex: 1; background: var(--green); opacity: 0.8; }
  .rows, .lines { display: flex; flex-direction: column; gap: 4px; width: 100%; }
  .rows i { height: 4px; }
  .lines i { height: 3px; background: var(--text-3); opacity: 0.6; }
  .dots { display: flex; flex-direction: column; gap: 4px; width: 100%; }
  .dots i { width: 5px; height: 5px; border-radius: 50%; box-shadow: 8px 0 0 -1px var(--line-strong), 14px 0 0 -1px var(--line-strong), 20px 0 0 -1px var(--line-strong); }
  .dots i.red { background: var(--red); }
  .dots i.amber { background: var(--amber); }
  .dots i.green { background: var(--green); }
  .badges { display: grid; grid-template-columns: repeat(2, 12px); gap: 4px; }
  .badges i { width: 12px; height: 12px; border-radius: var(--r-xs); opacity: 0.7; }
  .what { flex: 1; display: flex; flex-direction: column; min-width: 0; }
  .what .title { color: var(--text); font-size: 13px; }
  .what .blurb { color: var(--muted); font-size: 12px; }
  .mark { flex: none; width: 18px; text-align: center; color: var(--muted); font-size: 15px; }
  .tile.on .mark { color: var(--accent); }
</style>
