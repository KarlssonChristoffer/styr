<!-- The project page's API connections: which outside APIs this project's threads get the key for. Adding a connection and its key happens in Settings.
     Used by ProjectView.svelte. -->
<script lang="ts">
  import { quiet } from "../quiet";
  import Quiet from "./Quiet.svelte";
  // The project page's API connections: which outside APIs this project's threads get the key
  // for. Adding a connection and its key happens in Settings.
  import { onMount } from "svelte";
  import { api } from "../api";
  import { apis } from "../apis.svelte";
  import { hub } from "../state.svelte";
  import Switch from "./Switch.svelte";

  let { path }: { path: string } = $props();
  const data = $derived(apis.data);

  const same = (a: string, b: string) => a.replace(/[\\/]+$/, "").toLowerCase() === b.replace(/[\\/]+$/, "").toLowerCase();

  async function load() {
    await quiet(apis.load(), "load");
  }
  onMount(load);

  async function toggle(name: string, on: boolean) {
    if (!data) return;
    const prefs = $state.snapshot(data.prefs);
    const c = prefs.connections.find((x) => x.name === name)!;
    c.projects = on ? [...c.projects, path] : c.projects.filter((p) => !same(p, path));
    try {
      await api.setApis(prefs);
      await load();
    } catch (e) {
      hub.fail(e);
    }
  }
</script>

<section>
  <div class="card-head">
    <h2>APIs</h2>
    <button class="more" onclick={() => { hub.apiOpen = null; hub.openSettings("apis"); }}>Manage</button>
  </div>
  {#if !data?.prefs.connections.length}
    <Quiet text="No API connections" />
  {:else}
    {#each data.prefs.connections as c (c.name)}
      {@const key = data.status.find((s) => s.name === c.name)?.key}
      <div class="row">
        <span class="grow">
          <span class="health {apis.health(c)}"></span>
          <button class="t" data-tip="Open {c.label} in Settings" onclick={() => { hub.apiOpen = c.name; hub.openSettings("apis"); }}>{c.label}</button>
          <span class="m mono">{c.secret}</span>
          {#if !key}<span class="m warn">no key</span>{/if}
        </span>
        <Switch label="Use {c.label} in this project" on={c.projects.some((p) => same(p, path))} onchange={(v) => toggle(c.name, v)} />
      </div>
    {/each}
  {/if}
</section>

<style>
  section { background: var(--surface); border-radius: var(--r-lg); padding: 16px 18px 12px; min-width: 0; }
  .card-head { display: flex; align-items: center; gap: 10px; margin-bottom: 8px; }
  h2 { color: var(--text); font-size: 14px; font-weight: 500; margin: 0; }
  .more { margin-left: auto; color: var(--muted); font-size: 12px; }
  .more:hover { color: var(--text); }
  .none { color: var(--muted); font-size: 13px; margin: 6px 0 8px; line-height: 1.5; }
  .row { display: flex; align-items: center; gap: 10px; padding: 4px 0; }
  .grow { flex: 1; min-width: 0; display: flex; gap: 8px; align-items: baseline; overflow: hidden; }
  .t { color: var(--text-2); font-size: inherit; padding: 0; }
  .t:hover { color: var(--text); text-decoration: underline; }
  .health { width: 6px; height: 6px; border-radius: 50%; background: var(--line-strong); flex: none; align-self: center; }
  .health.working { background: var(--green); }
  .health.failing { background: var(--red); }
  .health.nokey { background: transparent; border: 1px solid var(--muted); }
  .m { color: var(--muted); font-size: 12px; }
  .warn { color: var(--amber); }
  .hint { margin: 6px 0 0; }
</style>
