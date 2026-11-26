<!-- System's icon on the rail: a 2 by 2 grid that plays a little when you open System. Each time it plays the next move (slide a step, spin, hop, a wave, a short change of shape) and ends with the tile in the top right filled in for as long as System is open. Hovering the button spreads the tiles a pixel. The bottom-right tile carries the state, so colour still only means state: red when something is broken, amber when something is left on (green breathing is supported but the rail shows running work on Inbox). Reduced motion skips the moves.
     Used by Rail.svelte. -->
<script lang="ts">
  // System's icon on the rail: a 2 by 2 grid that plays a little when you open System. Each time
  // it plays the next move (slide a step, spin, hop, a wave, a short change of shape) and ends
  // with the tile in the top right filled in for as long as System is open. Hovering the button
  // spreads the tiles a pixel. The bottom-right tile carries the state, so colour still only
  // means state: red when something is broken, amber when something is left on (green breathing
  // is supported but the rail shows running work on Inbox). Reduced motion skips the moves.
  import { untrack } from "svelte";

  type Tone = "" | "amber" | "red" | "working";
  /** `pulse` goes up on every click of the button, so the grid plays even when System is already open. */
  let { open = false, tone = "", size = 16, pulse = 0 }: { open?: boolean; tone?: Tone; size?: number; pulse?: number } = $props();

  type Box = { x: number; y: number; w: number; h: number; r: number; o: number };
  // Slots in the grid: 0 top left, 1 top right, 2 bottom left, 3 bottom right.
  const SLOT: [number, number][] = [[4, 4], [13, 4], [4, 13], [13, 13]];
  /** The slot a tile moves to when the grid turns a quarter clockwise. */
  const CW = [1, 3, 0, 2];
  const LIT = 1;
  const STATE = 3;
  const box = (s: number, r = 1.6, o = 1): Box => ({ x: SLOT[s][0], y: SLOT[s][1], w: 7, h: 7, r, o });
  const grid = (slots: number[]) => slots.map((s) => box(s));
  // Shapes the grid can briefly turn into, by slot: bento, wide bento, soft tiles with one round.
  const SHAPES: Box[][] = [
    [{ x: 4, y: 4, w: 7, h: 16, r: 1.6, o: 1 }, box(1), box(2, 1.6, 0), box(3)],
    [{ x: 4, y: 4, w: 16, h: 7, r: 1.6, o: 1 }, box(1, 1.6, 0), box(2), box(3)],
    [box(0, 2.6), box(1, 2.6), box(2, 2.6), box(3, 3.5)],
  ];
  const MOVES = ["slide", "spin", "hop", "wave", "shape"] as const;

  let slots = $state([0, 1, 2, 3]); // the slot each tile sits in
  let boxes = $state<Box[]>(grid([0, 1, 2, 3]));
  let spin = $state(0);
  let snap = $state(false); // turns the spin transition off while the grid is reset after a spin
  let wave = $state<number[]>([]);
  let move = 0;
  let run = 0; // bumped by each play, so a newer click stops the move still playing
  let shape = 0;

  const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
  const back = (t: number) => 1 + 2.5 * (t - 1) ** 3 + 1.5 * (t - 1) ** 2;
  const lerp = (a: number, b: number, t: number) => a + (b - a) * t;

  function tween(to: Box[], ms: number, mine = run): Promise<void> {
    const from = boxes.map((b) => ({ ...b }));
    const t0 = performance.now();
    return new Promise((done) => {
      const step = (now: number) => {
        if (mine !== run) return done();
        const t = Math.min((now - t0) / ms, 1);
        const e = back(t);
        const f = Math.min(t * 1.5, 1);
        boxes = from.map((a, i) => {
          const b = to[i];
          return { x: lerp(a.x, b.x, e), y: lerp(a.y, b.y, e), w: lerp(a.w, b.w, e), h: lerp(a.h, b.h, e), r: Math.max(0, lerp(a.r, b.r, e)), o: lerp(a.o, b.o, f) };
        });
        if (t < 1) requestAnimationFrame(step);
        else done();
      };
      requestAnimationFrame(step);
    });
  }

  async function play() {
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    // A click in the middle of a move stops it: the tiles settle in their slots and the next
    // move starts from there.
    const mine = ++run;
    const stale = () => mine !== run;
    snap = true;
    spin = 0;
    wave = [];
    boxes = grid(slots);
    await wait(20);
    if (stale()) return;
    snap = false;
    {
      const m = MOVES[move++ % MOVES.length];
      if (m === "slide") {
        slots = slots.map((s) => CW[s]);
        await tween(grid(slots), 360);
      } else if (m === "spin") {
        // Turning the grid a quarter clockwise leaves every tile in the next slot, so after the
        // turn the tiles take those slots and the turn is undone without a visible jump.
        spin = 90;
        await wait(430);
        if (stale()) return;
        snap = true;
        slots = slots.map((s) => CW[s]);
        boxes = grid(slots);
        spin = 0;
        await wait(30);
        snap = false;
      } else if (m === "hop") {
        const i = slots.indexOf(LIT);
        await tween(boxes.map((b, j) => (j === i ? { ...b, x: 14.5, y: 1.5 } : b)), 200);
        if (stale()) return;
        await tween(grid(slots), 280);
      } else if (m === "wave") {
        for (const s of [0, 1, 3, 2]) {
          wave = [...wave, slots.indexOf(s)];
          await wait(70);
          if (stale()) return;
        }
        await wait(90);
        if (stale()) return;
        wave = [];
      } else {
        const to = SHAPES[shape++ % SHAPES.length];
        await tween(slots.map((s) => to[s]), 260);
        await wait(240);
        if (stale()) return;
        await tween(grid(slots), 260);
      }
    }
  }

  // Play when System opens (by click, key or palette) and on every click of the button, but not
  // when the rail first draws with System already open.
  let was: boolean | null = null;
  let seen = 0;
  $effect(() => {
    const o = open;
    const p = pulse;
    untrack(() => {
      if ((was === false && o) || p !== seen) play();
      was = o;
      seen = p;
    });
  });
</script>

<svg class="sys" class:open viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
  <g class:snap style:transform="rotate({spin}deg)">
    {#each boxes as b, i (i)}
      <rect x={b.x} y={b.y} width={b.w} height={b.h} rx={b.r} opacity={b.o}
        class="s{slots[i]}" class:lit={(open && slots[i] === LIT) || wave.includes(i)}
        class:red={tone === "red" && slots[i] === STATE} class:amber={tone === "amber" && slots[i] === STATE}
        class:working={tone === "working" && slots[i] === STATE} />
    {/each}
  </g>
</svg>

<style>
  .sys { overflow: visible; display: block; }
  g { transform-origin: 12px 12px; transition: transform 0.42s var(--ease); }
  g.snap { transition: none; }
  rect {
    fill: currentColor; fill-opacity: 0; stroke: currentColor; stroke-width: 1.75;
    transform-box: fill-box; transform-origin: center;
    transition: fill-opacity var(--dur-move) var(--ease), stroke var(--dur-move) var(--ease), fill var(--dur-move) var(--ease), transform 0.22s var(--ease);
  }
  rect.lit { fill-opacity: 1; }
  rect.red { stroke: var(--red); fill: var(--red); fill-opacity: 1; }
  rect.amber { stroke: var(--amber); }
  rect.working { animation: breathe 2.4s var(--ease-move) infinite; }
  @keyframes breathe { 50% { stroke: var(--green); } }
  :global(.place:hover) rect.s0 { transform: translate(-1px, -1px); }
  :global(.place:hover) rect.s1 { transform: translate(1px, -1px); }
  :global(.place:hover) rect.s2 { transform: translate(-1px, 1px); }
  :global(.place:hover) rect.s3 { transform: translate(1px, 1px); }
  @media (prefers-reduced-motion: reduce) {
    g, rect { transition: none; }
    rect.working { animation: none; stroke: var(--green); }
  }
</style>
