<!-- The simulator big, in the right panel next to the thread: the same live picture as the floating phone, with the choice of device and the state of the stream in a bar above it.
     Used by App.svelte. -->
<script lang="ts">
  // The simulator big, in the right panel next to the thread: the same live picture as the floating
  // phone, with the choice of device and the state of the stream in a bar above it.
  import { sim } from "../simview.svelte";
  import { contextMenu } from "../menu.svelte";
  import Icon from "./Icon.svelte";
  import Select from "./Select.svelte";
  import SimScreen from "./SimScreen.svelte";

  let { udid, host = null }: { udid: string; host?: string | null } = $props();

  const view = $derived(sim.view(host));
  const running = $derived((view?.devices ?? []).filter((d) => d.booted || d.udid === udid));
  const options = $derived(
    (view?.devices ?? []).map((d) => ({ value: d.udid, label: d.name, hint: d.booted ? `${d.runtime} · running` : d.runtime })),
  );
  const live = $derived(view?.live);
  /** The Hub app menu: Hub's own iPhone app in this simulator, paired without typing a code. */
  function appMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const zoom = Number(document.documentElement.style.zoom) || 1;
    const thread = sim.shown?.thread;
    const run = (app: { action: "start" | "thread" | "tab"; thread?: string; tab?: number }) => () => void sim.app(host, udid, app);
    const tab = (label: string, n: number) => ({ label, run: run({ action: "tab", tab: n }) });
    contextMenu.show(Math.max(4, r.right / zoom - 250), r.bottom / zoom + 4, sim.noApp[host ?? ""]
      ? [{ label: "Needs the next version of Hub on this machine", disabled: true }]
      : [
          { label: "Hub's own app in this simulator", heading: true },
          { label: "Start paired with this machine", icon: "play", hint: "no sign-in", run: run({ action: "start" }) },
          thread ? { label: "Open this thread in the app", icon: "link", run: run({ action: "thread", thread }) } : undefined,
          null,
          { label: "Show", heading: true },
          tab("Inbox", 0),
          tab("Chats", 1),
          tab("Machines", 2),
          tab("Settings", 3),
        ]);
  }
  let value = $state("");
  $effect(() => {
    value = udid;
  });
</script>

<div class="panel">
  <div class="bar">
    <span class="name">Simulator</span>
    <Select size="small" bind:value {options} label="Simulator" onchange={(u) => void sim.switchTo(u)} />
    {#if host}<span class="host">{host}</span>{/if}
    <span class="grow"></span>
    {#if live?.phase === "live"}
      <span class="pill"><i></i>Live{live.fps ? ` · ${live.fps.toFixed(1)}/s` : ""}</span>
    {:else if live && live.phase !== "off"}
      <span class="pill dim">{live.phase === "error" ? "Stopped" : "Starting…"}</span>
    {/if}
    <button class="chip" data-tip="Hub's own app in this simulator" onclick={appMenu}><Icon name="phone" size={13} />Hub app ⌄</button>
    {#if sim.canTouch(host)}
      <button class="ghost" data-tip="Home" aria-label="Home" onclick={() => void sim.touch(host, udid, { button: "home" })}><Icon name="home" size={14} /></button>
      <button class="ghost" data-tip="App switcher" aria-label="App switcher" onclick={() => void sim.touch(host, udid, { button: "app_switcher" })}><Icon name="copy" size={14} /></button>
    {/if}
    {#if !sim.noShutdown[host ?? ""]}
      <button class="ghost" data-tip="Power off this simulator" aria-label="Power off" onclick={() => void sim.powerOff(host, udid)}><Icon name="power" size={14} /></button>
    {/if}
    <button class="ghost" data-tip="Back to a floating phone" aria-label="Float" onclick={() => sim.place("float")}><Icon name="panel" size={14} /></button>
    <button class="ghost" data-tip="Close; the simulator keeps running" aria-label="Close" onclick={() => void sim.close()}><Icon name="x" size={14} /></button>
  </div>
  <div class="stage">
    {#key udid}<div class="phone"><SimScreen {host} {udid} radius="7%" /></div>{/key}
  </div>
  {#if running.length === 0}<div class="foot">No simulator is running on this machine.</div>{/if}
</div>

<style>
  .panel { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .bar { display: flex; align-items: center; gap: 8px; padding: 10px 12px 10px 16px; border-bottom: 1px solid var(--line); min-height: 44px; }
  .name { color: var(--text); font-weight: 500; font-size: 14px; white-space: nowrap; flex: none; }
  .host { color: var(--muted); font-size: 12px; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .grow { flex: 1; }
  .pill { display: inline-flex; align-items: center; gap: 6px; height: 22px; padding: 0 8px; border-radius: 999px; font-size: 12px; color: var(--green); background: var(--surface); border: 1px solid var(--line); white-space: nowrap; }
  .pill i { width: 7px; height: 7px; border-radius: 50%; background: var(--green); }
  .pill.dim { color: var(--muted); }
  .chip { display: inline-flex; align-items: center; gap: 6px; height: 24px; padding: 0 8px; border-radius: var(--r-sm); font-size: 13px; color: var(--text-2); }
  .chip:hover { color: var(--text); background: var(--surface-hover); }
  .ghost { color: var(--muted); display: grid; place-items: center; padding: 4px; border-radius: var(--r-sm); }
  .ghost:hover { color: var(--text); background: var(--surface-hover); }
  .stage { flex: 1; min-height: 0; display: flex; justify-content: center; padding: 18px; overflow: hidden; }
  .phone { height: 100%; min-height: 0; aspect-ratio: 1206 / 2622; max-width: 100%; filter: drop-shadow(0 14px 30px rgba(0, 0, 0, 0.5)); }
  .phone :global(.screen) { height: 100%; aspect-ratio: auto; }
  .foot { padding: 10px 16px; border-top: 1px solid var(--line); color: var(--muted); font-size: 12px; }
</style>
