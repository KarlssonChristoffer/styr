<!-- A Linux machine joined the tailnet and is not in Hub yet: one click sets it up, and its row in Settings > Machines shows how that goes. "Not now" hides it here for good; it stays in Settings.
     Used by Home.svelte. -->
<script lang="ts">
  // A Linux machine joined the tailnet and is not in Hub yet: one click sets it up, and its row in
  // Settings > Machines shows how that goes. "Not now" hides it here for good; it stays in Settings.
  import { hub } from "../state.svelte";
  import type { TailnetPeer } from "../bindings/TailnetPeer";
  import Icon from "./Icon.svelte";

  const offered = $derived(hub.newMachines.filter((p) => !p.dismissed).slice(0, 3));
  let busy = $state<string | null>(null);

  async function setUp(p: TailnetPeer) {
    busy = p.name;
    try {
      await hub.setUpMachine({ name: p.name.toLowerCase(), ssh: p.ssh });
      hub.openSettings("machines");
    } catch (e) {
      hub.fail(e);
    } finally {
      busy = null;
    }
  }
</script>

{#each offered as p (p.name)}
  <div class="found">
    <Icon name="plus" size={13} />
    <span class="what"><b>{p.name}</b> joined your tailnet. Set it up to run threads there?</span>
    <button class="go" disabled={busy !== null} data-tip={`Over SSH to ${p.ssh}: installs Hub there if it has none, built on this machine`} onclick={() => setUp(p)}>
      {busy === p.name ? "Starting…" : "Set up"}
    </button>
    <button class="later" onclick={() => hub.dismissMachine(p.name).catch((e) => hub.fail(e))}>Not now</button>
  </div>
{/each}

<style>
  .found { display: flex; align-items: center; gap: 8px; background: var(--surface); border: 1px solid var(--line-strong); border-radius: var(--r-lg); padding: 8px 10px 8px 12px; margin-bottom: 12px; font-size: 13px; }
  .found :global(svg) { color: var(--muted); flex: none; }
  .what { flex: 1; min-width: 0; color: var(--text-3); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .what b { color: var(--text); font-weight: 500; }
  .go { color: var(--accent); background: var(--accent-bg); padding: 3px 10px; border-radius: var(--r-sm); font-size: 13px; }
  .go:disabled { opacity: 0.6; }
  .later { color: var(--muted); padding: 3px 8px; border-radius: var(--r-sm); font-size: 13px; }
  .later:hover { color: var(--text-2); background: var(--surface-hover); }
</style>
