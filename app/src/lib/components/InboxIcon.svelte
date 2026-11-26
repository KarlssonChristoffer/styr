<!-- Inbox's icon on the rail: three rows, each a small square and a line, like the thread list. Opening Inbox, and every later click on it, plays the next move (a new row lands on top, the rows slide in, the top one is ticked off, the bottom one jumps to the top, a wave, the lines are written out, the rows hop) and the top square stays filled while Inbox is open. Hovering spreads the rows a little. The top square carries the state instead of a dot: red when a thread failed, amber when one waits on you, green breathing while one works. A sibling of SystemIcon, built the same way; reduced motion skips the moves.
     Used by Rail.svelte. -->
<script lang="ts">
  // Inbox's icon on the rail: three rows, each a small square and a line, like the thread list.
  // Opening Inbox, and every later click on it, plays the next move (a new row lands on top, the
  // rows slide in, the top one is ticked off, the bottom one jumps to the top, a wave, the lines
  // are written out, the rows hop) and the top square stays filled while Inbox is open. Hovering
  // spreads the rows a little. The top square carries the state instead of a dot: red when a
  // thread failed, amber when one waits on you, green breathing while one works. A sibling of
  // SystemIcon, built the same way; reduced motion skips the moves.
  import { untrack } from "svelte";

  type Tone = "" | "amber" | "red" | "working";
  // `land` goes up when something was sent to Inbox from elsewhere (a fix thread from Problems):
  // a row lands on top each time.
  let { open = false, tone = "", size = 16, pulse = 0, land = 0 }: { open?: boolean; tone?: Tone; size?: number; pulse?: number; land?: number } = $props();

  type Row = { y: number; x: number; o: number; len: number; f: number };
  // Slots top to bottom; the last line is shorter, like a list that ends.
  const Y = [4, 10, 16];
  const LEN = [9, 9, 5];
  const MOVES = ["arrive", "slide", "done", "sort", "wave", "type", "hop"] as const;

  let slots = $state([0, 1, 2]); // the slot each row sits in
  const at = (s: number): Row => ({ y: Y[s], x: 0, o: 1, len: LEN[s], f: 0 });
  const home = () => slots.map(at);
  let rows = $state<Row[]>([0, 1, 2].map(at));
  let move = 0;
  let run = 0; // bumped by each play, so a newer click stops the move still playing

  const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
  const back = (t: number) => 1 + 2.5 * (t - 1) ** 3 + 1.5 * (t - 1) ** 2;
  const out = (t: number) => 1 - (1 - t) ** 3;
  const lin = (t: number) => t;
  const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
  /** The row sitting in each slot, top to bottom. */
  const order = () => [0, 1, 2].map((s) => slots.indexOf(s));

  function tween(to: Row[], ms: number, delays = [0, 0, 0], ease = back, mine = run): Promise<void> {
    const from = rows.map((r) => ({ ...r }));
    const t0 = performance.now();
    return new Promise((done) => {
      const step = (now: number) => {
        if (mine !== run) return done();
        let all = true;
        rows = from.map((a, i) => {
          const b = to[i];
          const t = Math.min(Math.max((now - t0 - delays[i]) / ms, 0), 1);
          if (t < 1) all = false;
          const e = ease(t);
          const f = Math.min(t * 1.5, 1);
          return { y: lerp(a.y, b.y, e), x: lerp(a.x, b.x, e), len: lerp(a.len, b.len, e), o: lerp(a.o, b.o, f), f: lerp(a.f, b.f, f) };
        });
        if (all) done();
        else requestAnimationFrame(step);
      };
      requestAnimationFrame(step);
    });
  }

  async function play(only?: (typeof MOVES)[number]) {
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    // A click in the middle of a move stops it: the rows settle in their slots and the next move
    // starts from there.
    const mine = ++run;
    const stale = () => mine !== run;
    rows = home();
    const m = only ?? MOVES[move++ % MOVES.length];
    const stagger = slots.map((s) => s * 60);
    if (m === "arrive") {
      // A new row lands on top and pushes the others down; the bottom one drops away.
      const last = order()[2];
      await tween(rows.map((r, i) => (i === last ? { ...r, y: 21, o: 0 } : at(slots[i] + 1))), 240);
      if (stale()) return;
      slots = slots.map((s) => (s + 1) % 3);
      rows = rows.map((r, i) => (i === last ? { ...at(0), y: -1, o: 0 } : r));
      await tween(home(), 300);
    } else if (m === "slide") {
      rows = rows.map((r) => ({ ...r, x: 7, o: 0 }));
      await tween(home(), 380, stagger);
    } else if (m === "done") {
      // The top row is ticked off and slides away, the rest move up, it comes back at the bottom.
      const top = order()[0];
      await tween(rows.map((r, i) => (i === top ? { ...r, f: 1 } : r)), 120, undefined, lin);
      if (stale()) return;
      await tween(rows.map((r, i) => (i === top ? { ...r, x: -6, o: 0, len: 0 } : r)), 200, undefined, lin);
      if (stale()) return;
      slots = slots.map((s) => (s + 2) % 3);
      await tween(rows.map((r, i) => (i === top ? { ...at(2), y: 21, o: 0, len: 0 } : at(slots[i]))), 220);
      if (stale()) return;
      await tween(home(), 260);
    } else if (m === "sort") {
      // The bottom row steps out, jumps to the top and settles first.
      const last = order()[2];
      await tween(rows.map((r, i) => (i === last ? { ...r, x: 5 } : r)), 150);
      if (stale()) return;
      slots = slots.map((s, i) => (i === last ? 0 : s + 1));
      await tween(rows.map((r, i) => (i === last ? { ...at(0), x: 5 } : at(slots[i]))), 280);
      if (stale()) return;
      await tween(home(), 200);
    } else if (m === "wave") {
      for (const i of order()) {
        await wait(70);
        if (stale()) return;
        rows = rows.map((r, j) => (j === i ? { ...r, f: 1 } : r));
      }
      await wait(110);
      if (stale()) return;
      await tween(home(), 200, undefined, lin);
    } else if (m === "type") {
      rows = rows.map((r) => ({ ...r, len: 0 }));
      await tween(home(), 320, slots.map((s) => s * 90), out);
    } else {
      await tween(rows.map((r) => ({ ...r, y: r.y - 2.2 })), 140, stagger, (t) => 1 - (1 - t) ** 2);
      if (stale()) return;
      await tween(home(), 240, stagger);
    }
  }

  // Play when Inbox opens (by click, key or palette) and on every click of the button, but not
  // when the rail first draws with Inbox already open.
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
  let landed = untrack(() => land);
  $effect(() => {
    const l = land;
    untrack(() => {
      if (l !== landed) play("arrive");
      landed = l;
    });
  });
</script>

<svg class="inbox" viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
  {#each rows as r, i (i)}
    <g class="row s{slots[i]}" opacity={r.o}>
      <rect x={3.5 + r.x} y={r.y} width="5" height="5" rx="1.8" style:fill-opacity={open && slots[i] === 0 ? 1 : r.f}
        class:red={tone === "red" && slots[i] === 0} class:amber={tone === "amber" && slots[i] === 0}
        class:working={tone === "working" && slots[i] === 0} />
      <path d="M{11.5 + r.x} {r.y + 2.5}h{Math.max(r.len, 0.01)}" />
    </g>
  {/each}
</svg>

<style>
  .inbox { overflow: visible; display: block; }
  rect, path { stroke: currentColor; stroke-width: 1.75; stroke-linecap: round; }
  rect { fill: currentColor; transition: stroke var(--dur-move) var(--ease), fill var(--dur-move) var(--ease); }
  path { fill: none; }
  g { transition: transform 0.22s var(--ease); }
  rect.red { stroke: var(--red); fill: var(--red); fill-opacity: 1 !important; }
  rect.amber { stroke: var(--amber); }
  rect.working { animation: breathe 2.4s var(--ease-move) infinite; }
  @keyframes breathe { 50% { stroke: var(--green); } }
  :global(.place:hover) g.s0 { transform: translateY(-1px); }
  :global(.place:hover) g.s2 { transform: translateY(1px); }
  @media (prefers-reduced-motion: reduce) {
    g, rect { transition: none; }
    rect.working { animation: none; stroke: var(--green); }
  }
</style>
