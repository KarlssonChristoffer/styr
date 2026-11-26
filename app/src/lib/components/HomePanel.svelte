<!-- The right panel on Home. Home is already the overview, so this only appears when something is worth tidying: servers left behind by finished threads, or a computer short of memory.
     Used by App.svelte. -->
<script lang="ts">
  // The right panel on Home. Home is already the overview, so this only appears when something
  // is worth tidying: servers left behind by finished threads, or a computer short of memory.
  import { api } from "../api";
  import type { Listener } from "../bindings/Listener";
  import { ago, bytes } from "../format";
  import { menu } from "../menu.svelte";
  import { serverMenu } from "../menus.svelte";
  import { hub } from "../state.svelte";

  const thread = (id: string | null) => hub.threads.find((t) => t.id === id);

  let stopping = $state<number | null>(null);
  async function stop(list: Listener[]) {
    try {
      for (const l of list) {
        if (l.pid == null) continue;
        stopping = l.pid;
        await api.stopProcess(l.pid);
      }
      await hub.refreshHost();
    } catch (e) {
      hub.fail(e);
    } finally {
      stopping = null;
    }
  }
</script>

<div class="panel">
  {#if hub.strained && hub.host}
    <section>
      <h4><span class="dot waiting_on_you"></span> Memory is nearly full</h4>
      <p class="say">
        {bytes(hub.host.mem_used)} of {bytes(hub.host.mem_total)} in use.
        <button class="link" onclick={() => { hub.view = "services"; hub.selected = null; }}>See what is running</button>
      </p>
    </section>
  {/if}

  {#if hub.strays.length}
    <section>
      <h4>
        Left running <span class="label">{hub.strays.length}</span>
        {#if hub.strays.length > 1}<button class="btn danger all" disabled={stopping !== null} onclick={() => stop(hub.strays)}>Stop all</button>{/if}
      </h4>
      <p class="say">Finished threads.</p>
      {#each hub.strays as l (`${l.port}-${l.pid}`)}
        {@const t = thread(l.thread_id)}
        <div class="row" data-tip={l.exe ?? ""} use:menu={() => serverMenu(l)}>
          <span class="port mono">:{l.port}</span>
          <span class="what">
            <span class="proc">{l.process ?? "unknown"}</span>
            {#if t}<button class="sub" onclick={() => hub.open(t.id)}>{t.title} · {t.archived ? "archived" : ago(t.updated_at)}</button>
            {:else}<span class="sub">Thread deleted</span>{/if}
          </span>
          <button class="btn danger" disabled={stopping === l.pid} onclick={() => stop([l])}>Stop</button>
        </div>
      {/each}
    </section>
  {/if}
</div>

<style>
  .panel { padding: 4px 18px 16px; overflow-y: auto; min-height: 0; flex: 1; }
  section { margin-top: 18px; }
  h4 { color: var(--text); font-size: 13px; font-weight: 500; margin: 0 0 4px; display: flex; gap: 8px; align-items: center; }
  .say { color: var(--muted); font-size: 13px; margin: 0 0 8px; line-height: 1.5; }
  .link { color: var(--accent); font-size: 13px; }
  .link:hover { text-decoration: underline; text-underline-offset: 2px; }
  .all { margin-left: auto; }
  .row { display: flex; align-items: center; gap: 10px; padding: 6px 0; border-bottom: 1px solid var(--line); }
  .row:last-child { border-bottom: 0; }
  .port { color: var(--text); background: var(--surface); padding: 1px 6px; border-radius: var(--r-sm); font-size: 12px; min-width: 58px; text-align: center; }
  .what { flex: 1; min-width: 0; display: flex; flex-direction: column; align-items: flex-start; }
  .proc { color: var(--text-2); font-size: 13px; }
  .sub { color: var(--muted); font-size: 12px; max-width: 100%; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; text-align: left; }
  button.sub:hover { color: var(--text); }
  .btn { height: 24px; padding: 0 8px; font-size: 12px; }
</style>
