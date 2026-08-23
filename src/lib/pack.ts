import type { PanelSpec } from "./types";

/** C-2 — the 2D bin-packing Deck.
 *
 *  Panels declare a width and height in grid cells; positions are **computed, never stored**
 *  (§7c). That is what makes a deck a list of panel ids rather than a saved pixel layout, and it
 *  is why adding a panel cannot break someone else's arrangement.
 *
 *  Skyline packing: track the current height of every column, and place each panel at the leftmost
 *  position where its full width fits at the lowest resulting top edge. First-fit-decreasing would
 *  pack denser but reorders panels, and a HUD whose panels move when you add one is a HUD you stop
 *  trusting — declaration order is preserved deliberately. */

export interface Placed {
  id: string;
  col: number;   // 1-based, for CSS grid-column
  row: number;   // 1-based, for CSS grid-row
  w: number;
  h: number;
}

export function pack(
  order: string[],
  specs: Record<string, PanelSpec>,
  cols: number,
): Placed[] {
  const heights = new Array<number>(Math.max(1, cols)).fill(0);
  const out: Placed[] = [];

  for (const id of order) {
    const spec = specs[id];
    if (!spec) continue;

    const w = Math.min(Math.max(spec.grid_data.min_w ?? 1, spec.grid_data.w), cols);
    const h = Math.max(1, spec.grid_data.h);

    // Lowest top edge at which a run of `w` columns fits; leftmost wins ties so the deck reads
    // left-to-right rather than scattering.
    let bestCol = 0;
    let bestTop = Infinity;
    for (let c = 0; c + w <= cols; c++) {
      let top = 0;
      for (let k = c; k < c + w; k++) top = Math.max(top, heights[k] ?? 0);
      if (top < bestTop) { bestTop = top; bestCol = c; }
    }
    if (!Number.isFinite(bestTop)) { bestTop = 0; bestCol = 0; }

    for (let k = bestCol; k < bestCol + w; k++) heights[k] = bestTop + h;
    out.push({ id, col: bestCol + 1, row: bestTop + 1, w, h });
  }
  return out;
}

/** How many grid columns the viewport supports. Twelve is the design's full width; narrower
 *  windows step down so panels never fall below their declared `min_w`. */
export function columnsFor(width: number): number {
  if (width >= 1500) return 12;
  if (width >= 1150) return 9;
  if (width >= 820) return 6;
  return 3;
}

/** One grid row's height in pixels. The handoff's tiles are wider than tall at the same cell
 *  count, so rows are deliberately smaller than columns. */
export const ROW_H = 34;
export const GAP = 14;

export interface Box { id: string; left: number; top: number; width: number; height: number; }

/** Grid units to pixels.
 *
 *  The Deck positions tiles **absolutely** rather than with CSS grid, which is not a stylistic
 *  choice: `left`/`top`/`width`/`height` are animatable, so a reflow glides instead of snapping,
 *  and a tile being dragged can leave the flow without the rest of the layout collapsing. */
export function boxes(placed: Placed[], cols: number, containerW: number): Box[] {
  const colW = (containerW - GAP * (cols + 1)) / cols;
  return placed.map((p) => ({
    id: p.id,
    left: GAP + (p.col - 1) * (colW + GAP),
    top: GAP + (p.row - 1) * (ROW_H + GAP),
    width: p.w * colW + (p.w - 1) * GAP,
    height: p.h * ROW_H + (p.h - 1) * GAP,
  }));
}

/** Total height the deck needs, so the scroll container is the right size. */
export function deckHeight(placed: Placed[]): number {
  const rows = placed.reduce((m, p) => Math.max(m, p.row - 1 + p.h), 0);
  return GAP + rows * (ROW_H + GAP);
}
