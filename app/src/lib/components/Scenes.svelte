<!-- Home's scenes: one fixed layout, a greeting with a line about now and the scenes as text links on top, the scene in the wide column and the same side column beside it (limits, machines, merge queue, update). Auto lets Hub pick the scene that fits what is going on (`scenes.ts`); a tab picks another and keeps it. On New thread the same page sits above the message box, `compact`: two blocks of the scene and no side column. Custom is the board you lay out yourself.
     Used by Home.svelte. -->
<script lang="ts">
  // Home's scenes: one fixed layout, a greeting with a line about now and the scenes as text links
  // on top, the scene in the wide column and the same side column beside it (limits, machines,
  // merge queue, update). Auto lets Hub pick the scene that fits what is going on (`scenes.ts`);
  // a tab picks another and keeps it. On New thread the same page sits above the message box,
  // `compact`: two blocks of the scene and no side column. Custom is the board you lay out yourself.
  import type { Snippet } from "svelte";
  import { away } from "../away.svelte";
  import { prefs, setPref } from "../prefs.svelte";
  import { dimmed, pickScene, SCENES, type Facts, type SceneId } from "../scenes";
  import { sim, stopSim, toggleSim } from "../sim.svelte";
  import { hub, waitsOnYou } from "../state.svelte";
  import SceneAway from "./scenes/SceneAway.svelte";
  import SceneLive from "./scenes/SceneLive.svelte";
  import SceneNeeds from "./scenes/SceneNeeds.svelte";
  import SceneProjects from "./scenes/SceneProjects.svelte";
  import SceneSide from "./scenes/SceneSide.svelte";
  import SceneToday from "./scenes/SceneToday.svelte";

  let {
    compact = false,
    summary,
    custom,
    tools,
  }: {
    compact?: boolean;
    summary: string;
    /** The board you lay out yourself, shown by the Custom tab. */
    custom?: Snippet;
    /** Customize, Undo and Reset, next to the greeting while Custom shows. */
    tools?: Snippet;
  } = $props();

  const startOfDay = $derived(new Date(hub.now).setHours(0, 0, 0, 0));
  const facts = $derived<Facts>({
    needs: hub.active.filter(waitsOnYou).length,
    running: hub.running.length,
    away: !!away.since && hub.active.some((t) => (t.returned_at ?? t.updated_at) > (away.since ?? 0) && t.status !== "running"),
    hour: new Date(hub.now).getHours(),
    today: hub.active.filter((t) => t.updated_at >= startOfDay).length,
  });
  const auto = $derived(pickScene(facts));
  const dim = $derived(dimmed(facts));
  const choice = $derived(prefs.homeScene === "custom" && compact ? "auto" : prefs.homeScene);
  // Simulating shows the Live scene while Auto is on, since that is where the made-up things are.
  const shown = $derived<SceneId | "custom">(choice === "auto" ? (sim.on ? "live" : auto) : choice);
  const set = (id: typeof prefs.homeScene) => setPref("homeScene", id);
  // Leaving the page ends the simulation: it is only ever on while you look at it.
  $effect(() => () => stopSim());
</script>

<div class="scenes" class:compact>
  <div class="hd">
    <div class="say">
      {#if summary}<p>{summary}</p>{/if}
    </div>
    {#if shown === "custom" && tools}{@render tools()}{/if}
    <div class="tabs" role="tablist">
      {#if !compact}
        <button class="sim" class:on={sim.on} aria-pressed={sim.on} onclick={toggleSim} data-tip="Fake data, this window only.">
          <i></i>{sim.on ? "Simulating" : "Simulate"}
        </button>
      {/if}
      <button class="auto" class:on={choice === "auto"} onclick={() => set("auto")}>Auto</button>
      {#each SCENES as s (s.id)}
        <button role="tab" aria-selected={shown === s.id} class:on={shown === s.id} class:off={dim.includes(s.id) && shown !== s.id} onclick={() => set(s.id)}>{s.name}</button>
      {/each}
      {#if !compact && custom}
        <button role="tab" aria-selected={shown === "custom"} class:on={shown === "custom"} onclick={() => set("custom")} data-tip="The board you lay out yourself">Custom</button>
      {/if}
    </div>
  </div>

  {#if shown === "custom" && custom}
    {@render custom()}
  {:else}
    <div class="grid">
      <div class="main">
        {#if shown === "needs"}<SceneNeeds />
        {:else if shown === "live"}<SceneLive />
        {:else if shown === "away"}<SceneAway />
        {:else if shown === "today"}<SceneToday />
        {:else}<SceneProjects />{/if}
      </div>
      {#if !compact}<SceneSide />{/if}
    </div>
  {/if}
</div>

<style>
  .scenes { container-type: inline-size; container-name: scenes; color: var(--text-2); font-size: 13px; }
  .hd { display: flex; align-items: flex-start; gap: 12px; margin: 8px 0 14px; flex-wrap: wrap; }
  .say { flex: 1; min-width: 220px; }
  p { margin: 0; color: var(--text-3); font-size: 13px; }
  .tabs { margin-left: auto; display: flex; flex-wrap: wrap; gap: 1px; align-items: center; margin-top: 1px; }
  .tabs button { font-size: 12px; color: var(--muted); padding: 3px 8px; border-radius: var(--r-sm); white-space: nowrap; }
  .tabs button:hover { color: var(--text-2); }
  .tabs button.on { color: var(--text); background: var(--surface-hover); }
  .tabs button.off { opacity: 0.45; }
  .tabs .sim { display: inline-flex; align-items: center; gap: 8px; margin-right: 8px; }
  .tabs .sim i { width: 7px; height: 7px; border-radius: 50%; box-shadow: inset 0 0 0 1.5px var(--muted); }
  .tabs .sim.on { background: var(--accent-bg); color: var(--accent); }
  .tabs .sim.on i { background: var(--green); box-shadow: none; animation: pulse 1.6s var(--ease-move) infinite; }
  .tabs .auto { font-size: 12px; margin-right: 4px; padding-left: 0; padding-right: 4px; }
  .tabs .auto.on { color: var(--accent); background: none; }

  .grid { display: grid; grid-template-columns: minmax(0, 1fr) 300px; gap: 20px; align-items: start; }
  .main { display: grid; grid-template-columns: repeat(12, minmax(0, 1fr)); gap: 14px 16px; align-items: start; min-width: 0; }
  @container scenes (max-width: 860px) {
    .grid { grid-template-columns: minmax(0, 1fr); }
  }
  @container scenes (max-width: 620px) {
    .main :global(section) { grid-column: span 12 !important; }
  }

  /* Pieces every scene shares: no strokes, a soft surface tint groups things, nothing above 18 px. */
  .scenes :global(.s12) { grid-column: span 12; }
  .scenes :global(.s7) { grid-column: span 7; }
  .scenes :global(.s6) { grid-column: span 6; }
  .scenes :global(.s5) { grid-column: span 5; }
  .scenes :global(section) { min-width: 0; }
  .scenes :global(.lbl) { display: flex; align-items: center; gap: 6px; color: var(--muted); font-size: 12px; font-weight: 500; margin: 0 0 6px; }
  .scenes :global(.lbl .n) { font-weight: 400; }
  .scenes :global(.lbl .r) { margin-left: auto; font-weight: 400; }
  .scenes :global(.blk) { background: var(--surface); border-radius: var(--r-md); padding: 6px 12px; min-width: 0; container-type: inline-size; container-name: blk; }
  .scenes :global(.blk.p) { padding: 10px 12px; }
  .scenes :global(.gap) { height: 10px; }
  .scenes :global(.side .lbl .n) { font-weight: 400; }
  .scenes :global(.b) { height: 22px; padding: 0 8px; border-radius: var(--r-sm); font-size: 12px; display: inline-flex; align-items: center; background: var(--surface-hover); color: var(--text-2); white-space: nowrap; }
  .scenes :global(.b.go) { background: var(--accent-bg); color: var(--accent); }
  .scenes :global(.mono) { font-family: var(--mono); font-size: 11px; color: var(--text-3); }
  .scenes :global(.hb) { display: grid; grid-template-columns: minmax(70px, 110px) 1fr 54px; gap: 6px 10px; align-items: center; font-size: 13px; color: var(--text-2); }
  .scenes :global(.hb .bar) { height: 5px; border-radius: 2px; background: var(--surface-hover); overflow: hidden; }
  .scenes :global(.hb .bar i) { display: block; height: 100%; border-radius: 2px; background: var(--accent); }
  .scenes :global(.hb .v) { text-align: right; color: var(--muted); font-variant-numeric: tabular-nums; }
  .scenes :global(.caption) { margin: 8px 0 0; font-size: 12px; color: var(--muted); }
  .scenes :global(.quiet) { color: var(--muted); font-size: 12px; margin: 0; }

  /* On New thread: only the blocks marked for it, two side by side, each cut short. */
  .compact .grid { grid-template-columns: minmax(0, 1fr); }
  .compact .main :global(section:not(.c)) { display: none; }
  .compact .main :global(section.c) { grid-column: span 6; }
  .compact .main :global(section.c .blk) { max-height: 132px; overflow: hidden; }
  @container scenes (max-width: 620px) {
    .compact .main :global(section.c) { grid-column: span 12 !important; }
  }
</style>
