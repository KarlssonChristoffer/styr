<!-- The phone floating over the chat: no frame, just the screen, with three quiet buttons in its top right corner (home, big, close) that show on hover. The screen takes touches like a phone; the phone is moved by its top edge, a thin grip that shows on hover.
     Used by App.svelte. -->
<script lang="ts">
  // The phone floating over the chat: no frame, just the screen, with three quiet buttons in its top
  // right corner (home, big, close) that show on hover. The screen takes touches like a phone; the
  // phone is moved by its top edge, a thin grip that shows on hover.
  import { sim } from "../simview.svelte";
  import Icon from "./Icon.svelte";
  import SimScreen from "./SimScreen.svelte";

  let { host = null, udid }: { host?: string | null; udid: string } = $props();

  const W = 188;
  let x = $state(Math.max(16, (typeof window === "undefined" ? 1200 : window.innerWidth) - W - 410));
  let y = $state(120);
  let drag: { dx: number; dy: number } | null = null;

  function down(e: PointerEvent) {
    if (!(e.target as HTMLElement).closest(".grip")) return;
    drag = { dx: e.clientX - x, dy: e.clientY - y };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    x = Math.min(Math.max(0, e.clientX - drag.dx), window.innerWidth - 60);
    y = Math.min(Math.max(0, e.clientY - drag.dy), window.innerHeight - 80);
  }
  const up = () => (drag = null);
</script>

<div class="float" role="group" aria-label="iPhone simulator" style:left="{x}px" style:top="{y}px" style:width="{W}px" onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={up}>
  <SimScreen {host} {udid} radius="12%" />
  <div class="grip" data-tip="Drag to move"><i></i></div>
  <div class="tools">
    {#if sim.canTouch(host)}
      <button data-tip="Home" aria-label="Home" onclick={() => void sim.touch(host, udid, { button: "home" })}><Icon name="home" size={13} /></button>
    {/if}
    <button data-tip="Open big in the side panel" aria-label="Open big" onclick={() => sim.place("panel")}><Icon name="expand" size={13} /></button>
    <button data-tip="Close; the simulator keeps running" aria-label="Close the phone" onclick={() => void sim.close()}><Icon name="x" size={13} /></button>
  </div>
</div>

<style>
  .float { position: fixed; z-index: 40; filter: drop-shadow(0 14px 30px rgba(0, 0, 0, 0.55)); }
  .grip { position: absolute; top: 0; left: 0; right: 64px; height: 26px; display: flex; justify-content: center; align-items: flex-start; padding-top: 8px; cursor: grab; touch-action: none; }
  .grip:active { cursor: grabbing; }
  .grip i { width: 34px; height: 4px; border-radius: 2px; background: rgba(255, 255, 255, 0.45); opacity: 0; transition: opacity var(--dur-fast) var(--ease); }
  .float:hover .grip i { opacity: 1; }
  .tools { position: absolute; top: 7px; right: 7px; z-index: 1; display: flex; gap: 4px; opacity: 0.28; transition: opacity var(--dur-fast) var(--ease); }
  .float:hover .tools { opacity: 1; }
  .tools button { width: 24px; height: 24px; border-radius: 50%; display: grid; place-items: center; color: #fff; background: rgba(20, 20, 20, 0.45); backdrop-filter: blur(8px); }
  .tools button:hover { background: rgba(20, 20, 20, 0.75); }
</style>
