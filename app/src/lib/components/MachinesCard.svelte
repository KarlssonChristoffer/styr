<!-- Machines: every machine Hub follows, online or not, and what runs on it. One row each; a click opens the machine's page. Counts come from the threads the window already has.
     Used by CardBody.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // Machines: every machine Hub follows, online or not, and what runs on it. One row each; a click
  // opens the machine's page. Counts come from the threads the window already has.
  import { machineGlyph } from "../refs";
  import { hub } from "../state.svelte";
  import HowCounted from "./HowCounted.svelte";
  import Ref from "./Ref.svelte";

  const rows = $derived(
    hub.hosts.map((h) => {
      const mine = hub.everything.filter((t) => hub.hostOf(t) === h.name);
      return { h, running: mine.filter((t) => t.status === "running").length, asks: mine.filter((t) => t.status === "waiting_on_you").length };
    }),
  );
  const online = $derived(hub.hosts.filter((h) => h.state === "online").length);
</script>

<HowCounted card="machines">
  <p>Machines this Hub follows. The count is threads working on it.</p>
</HowCounted>

{#if !rows.length}
  <Quiet text="No machines" />
{:else}
  <div class="list">
    {#each rows as { h, running, asks } (h.name)}
      <button class="line" class:off={h.state !== "online"} onclick={() => hub.openMachine(h.name)}>
        <Ref kind={machineGlyph(h.name, h.local)} label={h.name} online={h.state === "online"} />
        <span class="grow"></span>
        {#if h.state !== "online"}<span class="m">{h.state}</span>
        {:else if asks}<span class="m amber">{asks === 1 ? "1 needs you" : `${asks} need you`}</span>
        {:else if running}<span class="m green">{running} working</span>
        {:else}<span class="m">idle</span>{/if}
      </button>
    {/each}
  </div>
  <p class="caption"><em>{online}</em> of {hub.hosts.length} online</p>
{/if}

<style>
  .list { display: flex; flex-direction: column; }
  .line { display: flex; align-items: center; gap: 10px; width: 100%; min-width: 0; height: 32px; text-align: left; padding: 0; font-size: 13px; color: var(--text-2); }
  .line.off { opacity: 0.6; }
  .grow { flex: 1; }
  .m { color: var(--muted); font-size: 12px; white-space: nowrap; }
  .m.amber { color: var(--amber); }
  .m.green { color: var(--green); }
</style>
