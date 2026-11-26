<!-- Settings > Access: keys that let an agent outside Hub (Claude Code in a terminal, a bot) use Hub's tools, each with what it may do. A key is shown once, with the command that adds it.
     Used by Settings.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // Settings > Access: keys that let an agent outside Hub (Claude Code in a terminal, a bot) use
  // Hub's tools, each with what it may do. A key is shown once, with the command that adds it.
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { AgentAccess } from "../bindings/AgentAccess";
  import type { AgentKey } from "../bindings/AgentKey";
  import type { CreatedAgentKey } from "../bindings/CreatedAgentKey";
  import { ago } from "../format";
  import { copy } from "../menu.svelte";
  import { hub } from "../state.svelte";

  const levels: { id: AgentAccess; label: string; hint: string }[] = [
    { id: "read", label: "Read", hint: "Sees threads and engines. Changes nothing." },
    { id: "supervised", label: "Supervised", hint: "Also starts, messages and stops threads. They run with the engine's default permissions." },
    { id: "full", label: "Full", hint: "Also starts threads with any setting, the permission mode included." },
  ];
  const label = (a: AgentAccess) => levels.find((l) => l.id === a)?.label ?? a;

  let keys = $state<AgentKey[] | null>(null);
  let name = $state("");
  let access = $state<AgentAccess>("read");
  let made = $state<CreatedAgentKey | null>(null);
  let copied = $state(false);
  let making = $state(false);

  async function load() {
    try {
      keys = await api.agentKeys();
    } catch (e) {
      hub.fail(e);
    }
  }
  onMount(load);

  async function make() {
    making = true;
    try {
      made = await api.createAgentKey({ name: name.trim() || "Claude Code", access });
      copied = false;
      name = "";
      await load();
    } catch (e) {
      hub.fail(e);
    } finally {
      making = false;
    }
  }

  async function revoke(k: AgentKey) {
    try {
      await api.revokeAgentKey(k.id);
      if (made?.key.id === k.id) made = null;
      await load();
    } catch (e) {
      hub.fail(e);
    }
  }

  async function copyCommand() {
    if (!made) return;
    await copy(made.command);
    copied = true;
  }

  const inDays = (ms: number) => Math.max(0, Math.round((ms - Date.now()) / 86_400_000));
</script>

<h2>Agents outside Hub</h2>
<p class="lead">
  Keys for agents outside Hub.
</p>

<h2>New agent key</h2>
<section>
  <div class="line">
    <div class="what">
      <input class="field" bind:value={name} placeholder="Who uses it, e.g. Claude Code on PC" spellcheck="false"
        onkeydown={(e) => e.key === "Enter" && make()} />
      <div class="hint">{levels.find((l) => l.id === access)?.hint}</div>
    </div>
    <div class="control">
      <div class="seg">
        {#each levels as l (l.id)}
          <button class:active={access === l.id} onclick={() => (access = l.id)}>{l.label}</button>
        {/each}
      </div>
      <button class="btn primary" disabled={making} onclick={make}>{making ? "Making…" : "Make key"}</button>
    </div>
  </div>
  {#if made}
    <div class="made">
      <div class="hint">Run it where the agent runs. Shown once; lasts {inDays(made.key.expires_at)} days.</div>
      <div class="command">
        <code>{made.command}</code>
        <button class="btn small" onclick={copyCommand}>{copied ? "Copied" : "Copy"}</button>
      </div>
    </div>
  {/if}
</section>

<h2>Agent keys</h2>
<section class="list">
  {#each keys ?? [] as k (k.id)}
    <div class="change">
      <span class="subject">{k.name}</span>
      <span class="pill" class:warn={k.access === "full"}>{label(k.access)}</span>
      <span class="when">{k.last_used_at ? `used ${ago(k.last_used_at)}` : "not used yet"} · {inDays(k.expires_at)} days left</span>
      <button class="btn small danger" onclick={() => revoke(k)}>Revoke</button>
    </div>
  {:else}
    {#if keys}<Quiet text="No keys" />{:else}<Quiet kind="loading" />{/if}
  {/each}
</section>

<style>
  h2 { color: var(--text); font-size: 15px; font-weight: 600; margin: 34px 0 4px; }
  .lead { color: var(--text-3); margin: 0 0 8px; font-size: 14px; }
  section { margin-bottom: 4px; }
  .line { display: flex; align-items: center; gap: 32px; padding: 16px 0; }
  .what { flex: 1; min-width: 0; }
  .hint { color: var(--muted); font-size: 13px; margin-top: 4px; }
  .control { display: flex; align-items: center; gap: 8px; flex: none; }
  .field { width: 100%; height: 28px; padding: 0 8px; background: var(--bg); border: 1px solid var(--line-strong); border-radius: var(--r-md); font-size: 13px; outline: none; }
  .field:focus { border-color: var(--accent); }
  .seg { display: flex; background: var(--bg); border-radius: var(--r-md); padding: 2px; gap: 2px; }
  .seg button { height: 24px; padding: 0 10px; border-radius: var(--r-sm); color: var(--text-3); font-size: 13px; }
  .seg button:hover { color: var(--text); }
  .seg button.active { background: var(--surface-hover); color: var(--text); }
  .made { border-top: 1px solid var(--line); padding: 10px 0 12px; }
  .command { display: flex; align-items: center; gap: 10px; margin-top: 6px; background: var(--bg); border-radius: var(--r-md); padding: 8px 8px 8px 10px; }
  .command code { flex: 1; min-width: 0; font-size: 12px; color: var(--text-2); word-break: break-all; }
  .change { display: flex; align-items: center; gap: 12px; padding: 8px 0; min-height: 38px; }
  .change + .change { border-top: 1px solid var(--line); }
  .subject { flex: 1; min-width: 0; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .when { color: var(--muted); font-size: 12px; flex: none; }
  .pill { font-size: 12px; color: var(--text-3); background: var(--surface-hover); border-radius: var(--r-sm); padding: 1px 8px; white-space: nowrap; }
  .pill.warn { color: var(--amber); background: var(--amber-bg); }
  .btn.small { height: 24px; padding: 0 8px; font-size: 12px; }
</style>
