<!-- One row on another machine's page: how often it takes a new Hub version from this one ("As they come", "At most every 30 min"), so a day of merges does not restart it for each. Only for a machine added from the app that this Hub sends versions to (it has an SSH alias); one from hub.json is set there.
     Used by MachineView.svelte. -->
<script lang="ts">
  import { api } from "../api";
  import { hubdHas } from "../compat";
  import { hub } from "../state.svelte";
  import { pushGapChoices, pushGapOf } from "../pushGap";
  import type { HostInfo } from "../bindings/HostInfo";
  import Select from "./Select.svelte";

  let { name, h }: { name: string; h: HostInfo } = $props();

  /** Whether the hubd here can set it. compat-until: 2026-12-11 (a hubd before `hosts.settings`). */
  let can = $state<boolean | null>(null);
  hubdHas("hosts.settings").then((yes) => (can = yes));

  let busy = $state(false);
  const value = $derived(pushGapOf(h));

  async function pick(mins: number) {
    if (mins === value) return;
    busy = true;
    try {
      await api.setHost(name, { push_quiet_mins: mins });
      hub.hosts = await api.hosts();
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="row">
  <span>New versions</span>
  {#if can === false}
    <span class="muted">Needs the next version</span>
  {:else}
    <Select size="small" label="New versions" value={value} disabled={busy || can === null} options={pushGapChoices(value)} onchange={pick}
      tip={`How often ${name} takes a new Hub version from this computer. Update now still sends one at once.`} />
  {/if}
</div>

<style>
  .row { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--text-3); }
  .muted { color: var(--muted); }
</style>
