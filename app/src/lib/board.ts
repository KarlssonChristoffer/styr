// The Overview board's arithmetic, kept apart from Svelte so it stays plain. A card sits on a
// grid of COLS columns and ROW-pixel rows at x, y, w columns wide; its height is either chosen
// (h rows) or follows what it holds (h null, measured). Cards never overlap: `settle` drops each,
// in reading order, as high as the cards above it in its columns allow, like a wall of bricks.

/** Columns across the board. Fine enough that a card can be almost any width. */
export const COLS = 48;
/** Pixels in one grid row. Heights snap to this. */
export const ROW = 4;
/** The space between cards, in pixels. */
export const GAP = 10;
/** The narrowest and lowest a card can be pulled. */
export const MIN_W = 6;
export const MIN_H = 16;

/** A card's place. `with` lists the cards combined into it: they share its frame and spot, in
 *  this order under or beside it, and have no spot of their own until split off again. */
export type Spot = { id: string; x: number; y: number; w: number; h: number | null; with?: string[] };
export type Placed = Spot & { rows: number };

/** How many rows a card takes: its chosen height, or what it holds plus the gap below. */
export function rowsFor(s: Spot, measured: Record<string, number>): number {
  if (s.h != null) return s.h;
  const px = measured[s.id];
  return px ? Math.max(1, Math.ceil((px + GAP) / ROW)) : MIN_H;
}

/** Keeps a card on the board: whole numbers, inside the columns, not too small. */
export function clampSpot(s: Spot): Spot {
  const w = Math.max(MIN_W, Math.min(COLS, Math.round(s.w)));
  const x = Math.max(0, Math.min(COLS - w, Math.round(s.x)));
  const y = Math.max(0, Math.round(s.y));
  const h = s.h == null ? null : Math.max(MIN_H, Math.round(s.h));
  return s.with?.length ? { id: s.id, x, y, w, h, with: [...s.with] } : { id: s.id, x, y, w, h };
}

/** Lays the cards out with no overlap and no hole under any card. `first` (the one being
 *  dragged) wins a tie with a card at the same height, so dropping it on another puts it there
 *  and pushes the other down. */
export function settle(spots: Spot[], measured: Record<string, number>, first?: string): Placed[] {
  const order = [...spots].sort((a, b) => {
    const ay = a.y - (a.id === first ? 0.5 : 0);
    const by = b.y - (b.id === first ? 0.5 : 0);
    return ay - by || a.x - b.x;
  });
  const out: Placed[] = [];
  for (const s of order) {
    const rows = rowsFor(s, measured);
    let y = 0;
    for (const p of out) if (p.x < s.x + s.w && s.x < p.x + p.w) y = Math.max(y, p.y + p.rows);
    out.push({ ...s, y, rows });
  }
  return out;
}

/** Widths of the old S/M/L/Full cards, in columns. */
export const SIZE_COLS = { s: 16, m: 24, l: 32, full: 48 } as const;
export type Size = keyof typeof SIZE_COLS;

/** Puts cards from the old list (in order, each with a size) onto the board, left to right
 *  and row by row, the way the old page showed them. */
export function fromList(list: { id: string; size: Size }[]): Spot[] {
  const out: Spot[] = [];
  let x = 0;
  let row = 0;
  for (const c of list) {
    const w = SIZE_COLS[c.size] ?? 16;
    if (x + w > COLS) {
      x = 0;
      row++;
    }
    out.push({ id: c.id, x, y: row * 100, w, h: null });
    x += w;
  }
  return out;
}

/** Where a new card goes: the first place on the board, top first, where it fits beside the
 *  others; otherwise under everything. */
export function freeSpot(placed: Placed[], w: number): { x: number; y: number } {
  const bottom = placed.reduce((m, p) => Math.max(m, p.y + p.rows), 0);
  // The lowest card bottom in each column: a new card may sit where all its columns are free.
  const top = Array.from({ length: COLS }, (_, col) =>
    placed.filter((p) => p.x <= col && col < p.x + p.w).reduce((m, p) => Math.max(m, p.y + p.rows), 0),
  );
  let best: { x: number; y: number } | null = null;
  for (let x = 0; x + w <= COLS; x++) {
    const y = Math.max(...top.slice(x, x + w));
    if (y < bottom && (!best || y < best.y)) best = { x, y };
  }
  return best ?? { x: 0, y: bottom };
}
