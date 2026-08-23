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
