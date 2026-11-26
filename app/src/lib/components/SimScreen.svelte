<!-- The simulator's screen, bare: one picture asked for a few times a second (the Mac's hubd keeps the newest of the live stream, docs/ARCHITECTURE.md "The simulator view"). No frame around it.
     Used by SimFloat.svelte, SimPanel.svelte. -->
<script lang="ts">
  // The simulator's screen, bare: one picture asked for a few times a second (the Mac's hubd keeps
  // the newest of the live stream, docs/ARCHITECTURE.md "The simulator view"). No frame around it.
  import { api } from "../api";
  import { sim } from "../simview.svelte";

  let { host = null, udid, radius = "9%", touch = true }: { host?: string | null; udid: string; /** The screen's corner, as a share of its width. */ radius?: string; touch?: boolean } = $props();

  // A finger on the screen: pressing is a touch down, dragging moves it, letting go lifts it, so a
  // click is a tap and a drag is a swipe. Moves are collected for a moment and sent together, in order.
  type Pt = { phase: "begin" | "move" | "end"; x: number; y: number };
  let el: HTMLDivElement | undefined = $state();
  let down = false;
  let queue: Pt[] = [];
  let sending = false;
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function flush() {
    timer = undefined;
    if (sending || !queue.length) return;
    sending = true;
    const batch = queue.splice(0, 40);
    await sim.touch(host, udid, { touches: batch });
    sending = false;
    if (queue.length) void flush();
  }
  function push(p: Pt, now = false) {
    // Many moves in a row: only the newest of them matters once the next batch goes.
    if (p.phase === "move" && queue.length && queue[queue.length - 1].phase === "move" && queue.length > 8) queue.pop();
    queue.push(p);
    if (now) void flush();
    else timer ??= setTimeout(flush, 33);
  }
  function at(e: PointerEvent, phase: Pt["phase"]): Pt | null {
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { phase, x: Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)), y: Math.min(1, Math.max(0, (e.clientY - r.top) / r.height)) };
  }
  function pdown(e: PointerEvent) {
    if (!touch || !live || !sim.canTouch(host) || e.button !== 0) return;
    const p = at(e, "begin");
    if (!p) return;
    down = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    e.stopPropagation();
    push(p, true);
  }
  function pmove(e: PointerEvent) {
    if (!down) return;
    const p = at(e, "move");
    if (p) push(p);
  }
  function pup(e: PointerEvent) {
    if (!down) return;
    down = false;
    const p = at(e, "end");
    if (p) push(p, true);
  }

  let src = $state<string | null>(null);
  let failed = $state("");
  const phase = $derived(sim.view(host)?.live);
  const live = $derived(phase?.phase === "live");
  const waiting = $derived(
    failed ||
      (phase?.phase === "installing" ? "Fetching the streaming tool, once…" :
       phase?.phase === "booting" ? "Starting the simulator…" :
       phase?.phase === "error" ? (phase.detail ?? "The live picture stopped") :
       "Waiting for the first picture…"),
  );

  $effect(() => {
    const id = udid;
    const h = host;
    let stopped = false;
    let current: string | null = null;
    (async () => {
      while (!stopped) {
        const started = performance.now();
        try {
          const url = await api.simFrame(id, h);
          if (stopped) {
            if (url) URL.revokeObjectURL(url);
            break;
          }
          if (url) {
            const old = current;
            current = url;
            src = url;
            if (old) URL.revokeObjectURL(old);
          }
          failed = "";
        } catch (e) {
          failed = e instanceof Error ? e.message : String(e);
        }
        const spent = performance.now() - started;
        await new Promise((r) => setTimeout(r, Math.max(failed ? 1500 : 40, 200 - spent)));
      }
    })();
    return () => {
      stopped = true;
      if (current) URL.revokeObjectURL(current);
      src = null;
    };
  });
</script>

<div class="screen" class:touch={touch && live && sim.canTouch(host)} style:--r={radius} bind:this={el} role="application" aria-label="The iPhone simulator" onpointerdown={pdown} onpointermove={pmove} onpointerup={pup} onpointercancel={pup}>
  {#if src}
    <img {src} alt="The iPhone simulator" draggable="false" />
  {:else}
    <div class="wait">{waiting}</div>
  {/if}
</div>

<style>
  .screen { position: relative; aspect-ratio: 1206 / 2622; width: 100%; border-radius: var(--r); overflow: hidden; background: #000; }
  .screen.touch { cursor: pointer; touch-action: none; }
  img { display: block; width: 100%; height: 100%; object-fit: cover; user-select: none; -webkit-user-drag: none; pointer-events: none; }
  .wait { position: absolute; inset: 0; display: grid; place-items: center; padding: 16px; text-align: center; color: var(--muted); font-size: 12px; line-height: 1.4; }
</style>
