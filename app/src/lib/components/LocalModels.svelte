<!-- Settings > Engines > Providers: models served on your own machines (Ollama, LM Studio), found by asking each machine's hubd which local servers answer there (`agent_prefs::local_models`). Shown so you can see what is there; threads cannot run on them yet.
     Used by Settings.svelte. -->
<script lang="ts">
  // Settings > Engines > Providers: models served on your own machines (Ollama, LM Studio), found
  // by asking each machine's hubd which local servers answer there (`agent_prefs::local_models`).
  // Shown so you can see what is there; threads cannot run on them yet.
  import { untrack } from "svelte";
  import { api } from "../api";
  import type { LocalModels } from "../bindings/LocalModels";
  import { openUrl } from "../native";
  import { machineShort } from "../providers.svelte";
  import { hub } from "../state.svelte";
  import Icon from "./Icon.svelte";

  type Found = { machine: string; host: string | null; offline: boolean; models?: LocalModels; error?: string };
  let found = $state<Found[]>([]);
  let busy = $state(false);

  async function look() {
    busy = true;
    const machines = hub.hosts.length ? hub.hosts.map((h) => ({ machine: h.name, host: h.local ? null : h.name, offline: !h.local && h.state !== "online" })) : [{ machine: "This machine", host: null, offline: false }];
    found = machines.map((m) => ({ ...m }));
    await Promise.all(
      machines.map(async (m, i) => {
        if (m.offline) {
          found[i] = { ...m, error: "offline" };
          return;
        }
        try {
          found[i] = { ...m, models: await api.localModels(m.host) };
        } catch (e) {
          // A hubd from before this page answers that it has no such path.
          const why = e instanceof Error ? e.message : String(e);
          found[i] = { ...m, error: /not found|404|unexpected token|json/i.test(why) ? "its hubd is too old to look; update it" : why };
        }
      }),
    );
    busy = false;
  }

  // Once when the page opens; Look again asks again.
  $effect(() => {
    untrack(look);
  });

  const running = $derived(found.flatMap((f) => (f.models?.servers ?? []).filter((s) => s.running).map((s) => ({ f, s }))));
</script>

<div class="rack-row">
  <div class="rack-face">
    <span class="rack-badge"><Icon name="server" size={15} /></span>
    <span class="rack-main">
      <span class="rack-name">Local models <span class="pill">Finding only</span></span>
      <span class="rack-sub">
        {#if busy && !found.some((f) => f.models)}looking for Ollama and LM Studio…
        {:else if running.length}{running.length} running · threads cannot use them yet
        {:else}no Ollama or LM Studio running{/if}
      </span>
    </span>
    <span class="rack-end">
      {#each found as f (f.machine)}
        {@const on = (f.models?.servers ?? []).filter((s) => s.running)}
        <span class="led-box" data-tip={f.error ? `${f.machine}: ${f.error}` : on.length ? `${f.machine}: ${on.map((s) => `${s.label} (${s.models.length})`).join(", ")}` : `${f.machine}: none running`}>
          <span class="lamp" class:on={on.length > 0}></span>{machineShort(f.machine)}
        </span>
      {/each}
      <button class="btn" disabled={busy} onclick={look}>{busy ? "Looking…" : "Look again"}</button>
    </span>
  </div>
  {#if running.length || !busy}
    <div class="rack-more">
      {#each running as { f, s } (`${f.machine}/${s.id}`)}
        <span class="found"><span class="m">{f.machine}</span><span class="dim">{s.label}</span><span class="mono dim models">{s.models.length ? s.models.join(", ") : "no models pulled yet"}</span></span>
      {:else}
        <span class="dim">Get <button class="link" onclick={() => openUrl("https://ollama.com/download")}>Ollama</button> or <button class="link" onclick={() => openUrl("https://lmstudio.ai")}>LM Studio</button> to see your own models here.</span>
      {/each}
    </div>
  {/if}
</div>

<style>
  .found { display: flex; align-items: baseline; gap: 8px; min-width: 0; }
  .m { color: var(--text); flex: none; }
  .dim { color: var(--muted); }
  .models { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }
  .pill { font-size: 12px; color: var(--text-3); background: var(--surface-hover); border-radius: var(--r-sm); padding: 1px 8px; white-space: nowrap; }
  .link { color: var(--accent); font-size: inherit; padding: 0; }
  .link:hover { text-decoration: underline; }
</style>
