<!-- Overview's Customs card: how Customs is working, at a glance. Today's numbers, what is being reviewed now, and the last few things that happened; the rest is in Settings > Customs.
     Used by CardBody.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  import Quiet from "./Quiet.svelte";
  // Overview's Customs card: how Customs is working, at a glance. Today's numbers, what is being
  // reviewed now, and the last few things that happened; the rest is in Settings > Customs.
  import { onDestroy } from "svelte";
  import { api } from "../api";
  import { onAction } from "../overview.svelte";
  import type { CustomsActivity } from "../bindings/CustomsActivity";
  import type { CustomsItem } from "../bindings/CustomsItem";
  import { ago } from "../format";
  import { hub } from "../state.svelte";
  import { cameThrough, headline, hm, isToday, ring } from "../customsLog";

  let activity = $state<CustomsActivity | null>(null);
  let items = $state<CustomsItem[] | null>(null);
  let failed = $state(false);

  async function read() {
    try {
      [activity, items] = await Promise.all([api.customsActivity(), api.customs()]);
      failed = false;
    } catch {
      failed = true;
    }
  }
  read();
  const timer = poll(read, 15_000);
  onDestroy(() => timer());

  const today = $derived((activity?.entries ?? []).filter((e) => isToday(e.ts)));
  const n = (w: string) => today.filter((e) => e.what === w && e.by !== "you").length;
  const held = $derived((items ?? []).filter((i) => i.status === "held").length);
  const recent = $derived((activity?.entries ?? []).slice(0, 6));
  const open = () => hub.openSettings("customs");
  onAction("customs", "open", open);
</script>

{#if failed}
  <p class="quiet">Customs' log could not be read.</p>
{:else if !activity}
  <Quiet kind="loading" />
{:else}
  <div class="strip">
    <div><span class="v">{today.filter(cameThrough).length}</span><span class="l">today</span></div>
    <div><span class="v">{n("cleared")}</span><span class="l">cleared</span></div>
    <div><span class="v" class:amber={n("held")}>{n("held")}</span><span class="l">held</span></div>
    <div><span class="v">{n("refused")}</span><span class="l">refused</span></div>
  </div>
  {#if activity.reviewing.length}
    <div class="line now"><span class="pulse"></span><span class="what">Reviewing <span class="mono">{activity.reviewing[0].name}</span></span><span class="t">now</span></div>
  {/if}
  {#each recent as e (e.id)}
    <button class="line" onclick={open}>
      <span class="ring {ring(e)}"></span>
      {#if e.name}<span class="mono name">{e.name}</span>{/if}
      <span class="what">{e.name ? headline(e).toLowerCase() : `${headline(e)}: ${e.detail}`}</span>
      <span class="t">{isToday(e.ts) ? hm(e.ts) : ago(e.ts)}</span>
    </button>
  {:else}
    <Quiet text="Nothing yet" />
  {/each}
  {#if held}<p class="caption"><button class="held" onclick={open}>{held} held, waiting for you</button></p>{/if}
{/if}

<style>
  .held { color: var(--amber); font-size: 12px; background: none; padding: 0; }
  .mono { font-family: var(--mono); }
  .strip { display: flex; gap: 22px; margin-bottom: 8px; }
  .strip div { display: flex; flex-direction: column; }
  .v { color: var(--text); font-size: 22px; font-weight: 500; letter-spacing: -0.01em; font-variant-numeric: tabular-nums; }
  .v.amber { color: var(--amber); }
  .l { color: var(--muted); font-size: 12px; }
  .line { display: flex; align-items: center; gap: 8px; width: 100%; padding: 4px 0; border-top: 1px solid var(--line); font-size: 13px; background: none; border-radius: 0; text-align: left; min-width: 0; }
  button.line:hover .what { color: var(--text-2); }
  .name { color: var(--text); flex: none; max-width: 45%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .what { color: var(--text-3); flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .t { color: var(--muted); font-size: 12px; flex: none; font-variant-numeric: tabular-nums; }
  .ring { width: 8px; height: 8px; border-radius: 50%; border: 1.5px solid var(--muted); box-sizing: border-box; flex: none; }
  .ring.ok { border-color: var(--green); background: var(--green); }
  .ring.warn { border-color: var(--amber); }
  .ring.bad { border-color: var(--red); background: var(--red); }
  .ring.round { border-style: dotted; }
  .pulse { width: 8px; height: 8px; border-radius: 50%; background: var(--green); flex: none; animation: breathe 1.6s var(--ease-move) infinite; }
  @keyframes breathe { 50% { opacity: 0.35; } }
</style>
