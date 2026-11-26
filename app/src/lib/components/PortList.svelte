<!-- The ports listening on the status bar's machine, one tight line each: the port, what serves it and who started it (a thread, Hub's dev mode, Hub itself), with Open in the browser and Stop on hover. Stop takes a second click, so a slip never ends a server a thread still uses. The full detail is on Services (Control > Services).
     Used by BottomPanel.svelte, StatusBar.svelte. -->
<script lang="ts">
  import Quiet from "./Quiet.svelte";
  // The ports listening on the status bar's machine, one tight line each: the port, what serves
  // it and who started it (a thread, Hub's dev mode, Hub itself), with Open in the browser and Stop
  // on hover. Stop takes a second click, so a slip never ends a server a thread still uses. The
  // full detail is on Services (Control > Services).
  import type { Listener } from "../bindings/Listener";
  import { api } from "../api";
  import { ago } from "../format";
  import { openUrl } from "../native";
  import { hub } from "../state.svelte";
  import { bar } from "../statusbar.svelte";
  import Icon from "./Icon.svelte";

  let { ports, host }: { ports: Listener[]; host: string | null } = $props();

  let arming = $state<number | null>(null);
  let armTimer: ReturnType<typeof setTimeout> | undefined;

  function who(l: Listener): string {
    if (l.is_hub) return "Hub itself";
    if (l.origin === "dev_mode") return "Hub's dev mode";
    const t = l.thread_id ? hub.threads.find((x) => x.id === l.thread_id) : null;
    if (t) return t.title;
    if (l.cwd) return l.cwd.split(/[\\/]/).filter(Boolean).pop() ?? l.cwd;
    return "";
  }

  const what = (l: Listener) => (l.process ?? "").replace(/\.exe$/i, "") || "?";
  const url = (l: Listener) => `http://${host ?? "localhost"}:${l.port}/`;

  async function stop(l: Listener) {
    if (!l.pid) return;
    if (arming !== l.port) {
      arming = l.port;
      clearTimeout(armTimer);
      armTimer = setTimeout(() => (arming = null), 3000);
      return;
    }
    arming = null;
    try {
      await api.stopProcess(l.pid, host);
      await bar.loadPorts();
    } catch (e) {
      hub.fail(e);
    }
  }
</script>

<div class="ports">
  {#each ports as l (l.port)}
    <div class="row" class:dim={l.is_hub}>
      <button class="open" disabled={l.is_hub} data-tip={l.is_hub ? "Hub's own core" : `Open ${url(l)}`} onclick={() => openUrl(url(l)).catch((e) => hub.fail(e))}>
        <span class="port">{l.port}</span>
        <span class="what">{what(l)}</span>
        {#if who(l)}<span class="who">{who(l)}</span>{/if}
        {#if l.started_at && !l.is_hub}<span class="age">{ago(l.started_at)}</span>{/if}
      </button>
      {#if !l.is_hub}
        <button class="act" aria-label="Open in the browser" data-tip="Open in the browser" onclick={() => openUrl(url(l)).catch((e) => hub.fail(e))}><Icon name="external" size={12} /></button>
        {#if l.pid}
          <button class="act stop" class:armed={arming === l.port} aria-label="Stop" data-tip={arming === l.port ? "Click again to stop it" : l.thread_id ? "Stop (a thread started it)" : "Stop"} onclick={() => stop(l)}>
            {#if arming === l.port}Stop?{:else}<Icon name="stop" size={11} />{/if}
          </button>
        {/if}
      {/if}
    </div>
  {:else}
    <Quiet text="Nothing is listening" />
  {/each}
</div>

<style>
  .ports { display: flex; flex-direction: column; }
  .row { display: flex; align-items: center; gap: 2px; height: 26px; border-radius: var(--r-sm); padding-right: 2px; }
  .row:hover { background: var(--surface-hover); }
  .row.dim { opacity: 0.55; }
  .open { flex: 1; min-width: 0; display: flex; align-items: baseline; gap: 8px; height: 100%; padding: 0 6px; text-align: left; font-size: 13px; color: var(--text-2); }
  .open:disabled { cursor: default; }
  .port { flex: none; width: 44px; font-family: var(--mono); font-size: 12px; color: var(--accent); }
  .what { flex: none; color: var(--text-2); }
  .who { flex: 1; min-width: 0; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .age { flex: none; color: var(--muted); font-size: 12px; }
  .act { flex: none; height: 20px; min-width: 22px; padding: 0 4px; display: grid; place-items: center; border-radius: var(--r-xs); color: var(--muted); opacity: 0; font-size: 12px; }
  .row:hover .act, .act.armed { opacity: 1; }
  .act:hover { color: var(--text); background: var(--surface); }
  .act.armed { color: var(--red); background: var(--red-bg); }
</style>
