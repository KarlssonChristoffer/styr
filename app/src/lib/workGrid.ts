// The working indicator: the 2 by 2 grid of System's icon, playing one of several little loops
// while an agent works (the playful word in Minimal chat detail, `Transcript.svelte`). Every loop
// is a pure function of the time, and each one repeats within 5.2 s (`LOOP`), so any number of
// grids stay in step and the page needs no state of its own. Grey is the grid and green is only
// what is working, as everywhere in Hub.

export type Tile = {
  x: number; y: number; w: number; h: number; r: number; /** fill, 0..1 */ f: number; /** green in the outline, 0..1 */ g: number; o: number;
  /** A part of a tile at rest: drawn clipped to that slot's rounded tile, so a tile made of parts
   *  has the same round corners as a whole one and no seams. */
  clip?: number;
  /** Drawn without an outline: a part on its way, or a seated part. */
  ns?: boolean;
};
export type Shape = { sw: number; tiles: (t: number) => Tile[] };

/** Every loop repeats after this many milliseconds, or after a part of it. */
export const LOOP = 5200;

// Slots in the 24 by 24 box: 0 top left, 1 top right, 2 bottom left, 3 bottom right.
const SLOT: [number, number][] = [[4, 4], [13, 4], [4, 13], [13, 13]];
/** The slots clockwise. */
const RING = [0, 1, 3, 2];
const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
const cl = (x: number) => Math.max(0, Math.min(1, x));
/** 0 to 1 and back to 0 as `x` goes from 0 to 1. */
const bell = (x: number) => Math.sin(Math.PI * cl(x));
/** An ease that overshoots a little and settles, like the moves of System's icon. */
const back = (t: number) => 1 + 2.5 * (t - 1) ** 3 + 1.5 * (t - 1) ** 2;
const tile = (s: number, o: Partial<Tile> = {}): Tile => ({ x: SLOT[s][0], y: SLOT[s][1], w: 7, h: 7, r: 1.6, f: 0, g: 0, o: 1, ...o });
const grid = () => [0, 1, 2, 3].map((s) => tile(s));
/** The same tile scaled around its middle. */
const scaled = (b: Tile, sx: number, sy = sx): Tile => ({ ...b, x: b.x + (b.w * (1 - sx)) / 2, y: b.y + (b.h * (1 - sy)) / 2, w: b.w * sx, h: b.h * sy });
const HIDDEN: Tile = { x: 0, y: 0, w: 0, h: 0, r: 0, f: 0, g: 0, o: 0 };
/** A loop's tiles are always five: the four of the grid and one spare for a loop that needs it. */
const five = (t: Tile[]) => (t.length < 5 ? [...t, HIDDEN] : t);

const WIDE: Record<string, Partial<Tile>[]> = {
  tall: [{ x: 4, y: 4, w: 7, h: 16 }, {}, { o: 0 }, {}],
  wide: [{ x: 4, y: 4, w: 16, h: 7 }, { o: 0 }, {}, {}],
  soft: [{ r: 2.6 }, { r: 2.6 }, { r: 2.6 }, { r: 3.5 }],
};

// The fine grid inside each tile: 4 by 4 cells (8 by 8 over the grid), or 2 by 2 quarters.
const CS = 1.45;
const CG = 0.4;
const Q = 3.25;
const cellsOf = (bx: number, by: number): Tile[] => {
  const out: Tile[] = [];
  for (let j = 0; j < 4; j++) for (let i = 0; i < 4; i++) out.push({ x: bx + i * (CS + CG), y: by + j * (CS + CG), w: CS, h: CS, r: 0.5, f: 0.85, g: 1, o: 1 });
  return out;
};
const quartersOf = (bx: number, by: number): Tile[] =>
  [[0, 0], [1, 0], [0, 1], [1, 1]].map(([i, j]) => ({ x: bx + i * (Q + 0.5), y: by + j * (Q + 0.5), w: Q, h: Q, r: 0.9, f: 0.85, g: 1, o: 1 }));
const slotOf = (x: number, y: number) => (x >= 12 ? 1 : 0) + (y >= 12 ? 2 : 0);
/** A part at rest grows over the gaps, and a little over its neighbour so their soft edges leave
 *  no seam, and is clipped to its tile, so the tile reads whole. */
const seat = (c: Tile, gap: number): Tile => ({ ...c, w: c.w + gap + 0.35, h: c.h + gap + 0.35, r: 0, clip: slotOf(c.x, c.y), ns: true });
/** How long one move of the parted loops takes: four moves make a loop. */
const STEP = LOOP / 4;
/** Three filled tiles and a gap. One tile at a time moves into the gap in parts, the parts nearest
 *  the gap first, and the parts become one tile again where they land. */
function parted(t: number, split: (x: number, y: number) => Tile[], stagger: number, dur: number, gap: number): Tile[] {
  const n = Math.floor(t / STEP) % 4;
  let pos = [0, 1, 2];
  let hole = 3;
  for (let j = 0; j < n; j++) {
    const i = pos.indexOf((hole + 3) % 4);
    pos[i] = hole;
    hole = (hole + 3) % 4;
  }
  const mover = pos.indexOf((hole + 3) % 4);
  const p = (t % STEP) / STEP;
  const out: Tile[] = [];
  pos.forEach((q, i) => {
    if (i !== mover) return void out.push(tile(RING[q], { f: 0.85, g: 1 }));
    const [bx, by] = SLOT[RING[hole]];
    const A = split(...SLOT[RING[q]]);
    const B = split(bx, by);
    const order = A.map((c, k) => [k, Math.hypot(c.x - bx, c.y - by)]).sort((u, v) => u[1] - v[1]).map((u) => u[0]);
    // Before the first part leaves and after the last lands it is one whole tile, not parts.
    if (p <= 0) return void out.push(tile(RING[q], { f: 0.85, g: 1 }));
    if (cl((p - (A.length - 1) * stagger) / dur) >= 0.8) return void out.push(tile(RING[hole], { f: 0.85, g: 1 }));
    order.forEach((k, rank) => {
      const f = cl((p - rank * stagger) / dur);
      const e = back(f);
      if (f <= 0) out.push(seat(A[k], gap));
      // A part close to where it lands takes its seat, so a landing tile never shows the grid.
      else if (f >= 0.8) out.push(seat(B[k], gap));
      else out.push({ ...A[k], x: lerp(A[k].x, B[k].x, e), y: lerp(A[k].y, B[k].y, e), w: A[k].w + gap + 0.35, h: A[k].h + gap + 0.35, r: gap > 0.45 ? 1.2 : 0.55, f: 1, ns: true });
    });
  });
  return out;
}

export const SHAPES = {
  /** Three filled tiles and a gap; one tile pours into the gap cell by cell and is whole again where it lands. */
  pour: { sw: 1.5, tiles: (t) => parted(t, cellsOf, 0.018, 0.38, CG) },
  /** The same, with the tile crumbling into quarters on the way. */
  crumble: { sw: 1.5, tiles: (t) => parted(t, quartersOf, 0.1, 0.45, 0.5) },
  /** The tiles light one after another clockwise, each leaving a fading trace. */
  orbit: { sw: 1.7, tiles: (t) => five([0, 1, 2, 3].map((s) => {
    const d = ((((t / 2600) * 4 - RING.indexOf(s)) % 4) + 4) % 4;
    const l = Math.max(0, 1 - d / 2.4) ** 1.2;
    return tile(s, { f: l, g: l });
  })) },
  /** Three tiles and a gap: one tile slides into the gap at a time, clockwise (System's "slide"). */
  puzzle: { sw: 1.7, tiles: (t) => {
    const n = Math.floor(t / 650) % 4;
    let pos = [0, 1, 2];
    let gap = 3;
    for (let j = 0; j < n; j++) {
      const i = pos.indexOf((gap + 3) % 4);
      pos[i] = gap;
      gap = (gap + 3) % 4;
    }
    const mover = pos.indexOf((gap + 3) % 4);
    const f = cl(((t % 650) / 650) / 0.65);
    const e = back(f);
    return five(pos.map((p, i) => {
      if (i !== mover) return tile(RING[p]);
      const a = SLOT[RING[p]];
      const b = SLOT[RING[gap]];
      return tile(RING[p], { x: lerp(a[0], b[0], e), y: lerp(a[1], b[1], e), f: 0.3 * bell(f), g: bell(f) });
    }));
  } },
  /** The grid changes shape, tall, wide, soft, and back (System's "shape"); the corner tile breathes green. */
  breathe: { sw: 1.7, tiles: (t) => {
    const L = LOOP / 6;
    const i = Math.floor(t / L) % 6;
    const e = back(cl((t % L) / 300));
    const kinds = [null, "tall", null, "wide", null, "soft"] as const;
    const state = (k: number) => (kinds[k] ? WIDE[kinds[k]!].map((o, s) => tile(s, o)) : grid());
    const a = state((i + 5) % 6);
    const b = state(i);
    return five(a.map((A, k) => {
      const B = b[k];
      return { x: lerp(A.x, B.x, e), y: lerp(A.y, B.y, e), w: lerp(A.w, B.w, e), h: lerp(A.h, B.h, e), r: Math.max(0, lerp(A.r, B.r, e)),
        o: lerp(A.o, B.o, cl(((t % L) / 300) * 1.5)), f: 0, g: k === 3 ? 0.5 + 0.5 * Math.sin((t / 2600) * 2 * Math.PI) : 0 };
    }));
  } },
  /** One green tile hops from slot to slot around the grid (System's "hop"). */
  hop: { sw: 1.7, tiles: (t) => {
    const k = Math.floor(t / 650) % 4;
    const f = cl((t % 650) / 450);
    const e = back(f);
    const a = SLOT[RING[k]];
    const b = SLOT[RING[(k + 1) % 4]];
    return [...grid(), tile(0, { x: lerp(a[0], b[0], e), y: lerp(a[1], b[1], e) - 4 * bell(f), f: 1, g: 1 })];
  } },
  /** The whole grid turns a quarter at a time with a little bounce (System's "spin"). */
  turn: { sw: 1.7, tiles: () => five([tile(0), tile(1, { f: 1, g: 1 }), tile(2), tile(3)]),
    turn: (t) => 90 * (Math.floor(t / 1300) + back(cl((t % 1300) / 520))) },
  /** Four small green dots in a grid, blinking on the diagonal: today's three dots, as a grid. */
  micro: { sw: 0, tiles: (t) => {
    const p = (a: number) => 0.2 + 0.8 * (0.5 + 0.5 * Math.sin((t / 1300) * 2 * Math.PI + a));
    const d = (x: number, y: number, a: number): Tile => ({ x, y, w: 8, h: 8, r: 2, f: 1, g: 1, o: p(a) });
    return five([d(3, 3, 0), d(13, 3, Math.PI), d(3, 13, Math.PI), d(13, 13, 0)]);
  } },
  /** The tiles fill with green one by one, then empty in the same order. */
  fill: { sw: 1.7, tiles: (t) => {
    const tm = t % 2600;
    return five([0, 1, 2, 3].map((s) => {
      const k = RING.indexOf(s);
      const f = cl((tm - k * 325) / 220) - cl((tm - (4 + k) * 325) / 220);
      return tile(s, { f, g: f });
    }));
  } },
  /** The tiles flip like cards, one after another: empty ones turn green, then back. */
  flip: { sw: 1.7, tiles: (t) => {
    const tm = t % 2600;
    return five([0, 1, 2, 3].map((s) => {
      const k = RING.indexOf(s);
      const pa = cl((tm - k * 150) / 450);
      const pb = cl((tm - 1300 - k * 150) / 450);
      const on = pa > 0.5 && pb <= 0.5 ? 1 : 0;
      return scaled(tile(s, { f: on, g: on }), Math.max(Math.abs(Math.cos(Math.PI * pa)) * Math.abs(Math.cos(Math.PI * pb)), 0.02), 1);
    }));
  } },
  /** The tiles bounce up one after another, a wave of hops. */
  bounce: { sw: 1.7, tiles: (t) => {
    const tm = t % 1300;
    return five([0, 1, 2, 3].map((s) => {
      const b = bell((tm - RING.indexOf(s) * 140) / 520);
      return tile(s, { y: SLOT[s][1] - 4.5 * b, f: 0.25 * b, g: b });
    }));
  } },
  /** Two neighbouring tiles trade places and pass each other, once round the grid. */
  swap: { sw: 1.7, tiles: (t) => {
    const j = Math.floor(t / 650) % 4;
    const p = cl((t % 650) / 520);
    const e = back(p);
    const [a, b] = [[0, 1], [1, 3], [3, 2], [2, 0]][j];
    const horizontal = SLOT[a][1] === SLOT[b][1];
    const out = grid();
    const move = (from: number, to: number, side: number) => {
      const A = SLOT[from];
      const B = SLOT[to];
      const bump = 4 * bell(p) * side;
      return tile(from, { x: lerp(A[0], B[0], e) + (horizontal ? 0 : bump), y: lerp(A[1], B[1], e) + (horizontal ? bump : 0), f: 0.25 * bell(p), g: bell(p) });
    };
    out[a] = move(a, b, -1);
    out[b] = move(b, a, 1);
    return five(out);
  } },
  /** The tiles shrink to green dots and grow back, one after another. */
  shrink: { sw: 1.7, tiles: (t) => {
    const tm = t % 1300;
    return five([0, 1, 2, 3].map((s) => {
      const b = bell((tm - RING.indexOf(s) * 140) / 600);
      return scaled(tile(s, { r: lerp(1.6, 3.5, b), f: b * b, g: b }), 1 - 0.62 * b);
    }));
  } },
} satisfies Record<string, Shape & { turn?: (t: number) => number }>;

export type WorkShape = keyof typeof SHAPES;
export const WORK_SHAPES = Object.keys(SHAPES) as WorkShape[];
export const WORK_SHAPE_NAMES: Record<WorkShape, string> = {
  pour: "Pour", crumble: "Crumble", orbit: "Orbit", puzzle: "Puzzle", breathe: "Breathe", hop: "Hop", turn: "Turn", micro: "Micro",
  fill: "Fill", flip: "Flip", bounce: "Bounce", swap: "Swap", shrink: "Shrink",
};

/** The loop a turn uses: the one chosen in Settings, or with "random" one drawn for each turn.
 *  `seed` is the thread plus the turn, so the loop stays the same while the turn runs and two
 *  threads working at once do not all play the same. */
export function workShape(seed: string, choice: string): WorkShape {
  if (choice in SHAPES) return choice as WorkShape;
  let h = 7;
  for (const c of seed) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return WORK_SHAPES[h % WORK_SHAPES.length];
}
