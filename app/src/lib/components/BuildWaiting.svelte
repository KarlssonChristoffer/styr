<!-- Settings > Updates > "Waiting to build": what is merged and not yet in the running Hub, each change marked Rust (a full build) or pages, how far the count is from the one that starts the build, and Build now. Pages never wait; what is held is the Rust (`update.build_mode`).
     Used by Settings.svelte. -->
<script lang="ts">
  import { poll } from "../poll";
  // Settings > Updates > "Waiting to build": what is merged and not yet in the running Hub, each
  // change marked Rust (a full build) or pages, how far the count is from the one that starts
  // the build, and Build now. Pages never wait; what is held is the Rust (`update.build_mode`).
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { WaitingChange } from "../bindings/WaitingChange";
  import { hubdHas } from "../compat";
  import { clock, shortVersion } from "../format";
  import { hub } from "../state.svelte";

  let list = $state<WaitingChange[]>([]);
  let here = $state(false);
  const u = $derived(hub.update);
  const wait = $derived(u?.waiting ?? null);
  const held = $derived(!!(u?.waiting || u?.waiting_until));
  const rust = $derived(list.filter((c) => c.rust).length);
  const goal = $derived(wait?.goal ?? null);

  async function load() {
    try {
      list = await api.updateWaiting();
    } catch {
      list = [];
    }
  }
  onMount(() => {
    let alive = true;
    hubdHas("update.modes").then((v) => { // compat-until: 2026-12-03
      if (!alive || !v) return;
      here = true;
      void load();
    });
    const t = poll(() => here && void load(), 20_000);
    return () => {
      alive = false;
      t();
    };
  });
  // A new version on the way or a merge that changed what waits: read the list again.
  $effect(() => {
    void u?.target;
    void u?.waiting?.total;
    void u?.version;
    if (here) void load();
  });

  async function buildNow() {
    try {
      await api.buildUpdateNow();
      hub.tell("Building it now");
    } catch (e) {
      hub.fail(e);
    }
  }
  const day = (ms: number) => clock(ms);
</script>

{#if here && list.length}
  <h2>
    Waiting to build
    {#if held}<button class="btn" onclick={buildNow}>Build now</button>{/if}
  </h2>
  <div class="foot">
    {list.length} merged since <span class="mono">{shortVersion(u?.version, "")}</span> · {rust === 0 ? "no Rust" : `${rust} ${rust === 1 ? "change" : "changes"} to Rust`}{#if rust < list.length}; pages never wait{/if}
  </div>
  {#if goal}
    <div class="meter" data-tip="The build starts at {goal} Rust changes">
      {#each Array(Math.min(goal, 16)) as _, i (i)}<i class:full={i < rust}></i>{/each}
      <span>{rust} of {goal}{u?.waiting_until ? ` · builds at ${clock(u.waiting_until)} at the latest` : ""}</span>
    </div>
  {:else if u?.waiting_until}
    <div class="foot">Builds at {clock(u.waiting_until)}</div>
  {:else if wait?.mode === "manual"}
    <div class="foot">Builds when you press Build now.</div>
  {/if}
  <div class="list">
    {#each list.slice(0, 30) as c (c.sha)}
      <div class="row">
        <span class="kind" class:rust={c.rust}>{c.rust ? "Rust" : "Pages"}</span>
        <span class="title">{c.title}</span>
        <span class="when">{c.pr ? `#${c.pr} · ` : ""}{day(c.at)}</span>
      </div>
    {/each}
  </div>
{/if}

<style>
  h2 { display: flex; justify-content: space-between; align-items: center; color: var(--text); font-size: 15px; font-weight: 600; margin: 34px 0 4px; }
  .btn { height: 26px; padding: 0 11px; border-radius: var(--r-md); background: var(--surface-hover); color: var(--text-2); font-size: 13px; font-weight: 400; }
  .btn:hover { color: var(--text); }
  .foot { color: var(--muted); font-size: 12px; padding: 0 0 8px; }
  .meter { display: flex; align-items: center; gap: 4px; padding: 2px 0 10px; }
  .meter i { width: 22px; height: 6px; border-radius: 3px; background: var(--line-strong); }
  .meter i.full { background: var(--text-3); }
  .meter span { color: var(--muted); font-size: 13px; margin-left: 8px; }
  .row { display: flex; align-items: center; gap: 12px; min-height: 34px; border-top: 1px solid var(--line); font-size: 13px; }
  .kind { width: 48px; flex: none; color: var(--muted); font-size: 12px; }
  .kind.rust { color: var(--amber); }
  .title { flex: 1; min-width: 0; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .when { flex: none; color: var(--muted); font-size: 12px; white-space: nowrap; }
</style>
