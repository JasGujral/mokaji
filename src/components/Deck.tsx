import { useCallback, useEffect, useRef, useState } from "react";
import { boxes, columnsFor, deckHeight, GAP, pack, ROW_H } from "../lib/pack";
import type { PanelSpec } from "../lib/types";
import { Tile } from "./Tile";

export interface Size { w: number; h: number }

/** The Deck: a 2D bin-packed grid of draggable, resizable tiles.
 *
 *  Reordering is by drag-over rather than drop, so the layout reflows *under the cursor* and you
 *  can see where a panel will land before letting go — the packer runs on every swap. A throttle
 *  stops a slow diagonal drag from thrashing the order; the prototype uses 80 ms and so does this. */
export function Deck({
  order, specs, sizes, onReorder, onResize, onClose, render,
}: {
  order: string[];
  specs: Record<string, PanelSpec>;
  sizes: Record<string, Size>;
  onReorder: (dragged: string, target: string) => void;
  onResize: (id: string, size: Size) => void;
  onClose: (id: string) => void;
  render: (id: string) => React.ReactNode;
}) {
  const ref = useRef<HTMLDivElement | null>(null);
  const [width, setWidth] = useState(1200);
  const [dragging, setDragging] = useState<string | null>(null);
  const grabbed = useRef(false);
  const lastSwap = useRef(0);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => setWidth(el.clientWidth);
    measure();
    if (typeof ResizeObserver !== "undefined") {
      const ro = new ResizeObserver(measure);
      ro.observe(el);
      return () => ro.disconnect();
    }
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, []);

  const cols = columnsFor(width);
  const colW = (width - GAP * (cols + 1)) / cols;

  // Sizes the user set override the manifest's defaults; the manifest still owns min_w.
  const effective: Record<string, PanelSpec> = {};
  for (const [id, spec] of Object.entries(specs)) {
    const s = sizes[id];
    effective[id] = s ? { ...spec, grid_data: { ...spec.grid_data, w: s.w, h: s.h } } : spec;
  }

  const placed = pack(order, effective, cols);
  const laid = boxes(placed, cols, width);
  const height = deckHeight(placed);

  const startResize = useCallback(
    (id: string, e: React.PointerEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const spec = effective[id];
      if (!spec) return;
      const startX = e.clientX;
      const startY = e.clientY;
      const startW = spec.grid_data.w;
      const startH = spec.grid_data.h;
      const minW = spec.grid_data.min_w ?? 1;

      const move = (ev: PointerEvent) => {
        const dw = Math.round((ev.clientX - startX) / (colW + GAP));
        const dh = Math.round((ev.clientY - startY) / (ROW_H + GAP));
        onResize(id, {
          w: Math.max(minW, Math.min(cols, startW + dw)),
          h: Math.max(3, startH + dh),
        });
      };
      const up = () => {
        window.removeEventListener("pointermove", move);
        window.removeEventListener("pointerup", up);
      };
      window.addEventListener("pointermove", move);
      window.addEventListener("pointerup", up);
    },
    [effective, colW, cols, onResize],
  );

  return (
    <div className="deck" ref={ref}>
      <div style={{ position: "relative", height }}>
        {laid.map((box) => {
          const spec = effective[box.id];
          if (!spec) return null;
          return (
            <Tile
              key={box.id}
              box={box}
              title={spec.name}
              dragging={dragging === box.id}
              onGrab={(g) => { grabbed.current = g; }}
              onDragStart={(e) => {
                // Only the bar starts a drag. A panel of tasks is a place you select text.
                if (!grabbed.current) { e.preventDefault(); return; }
                setDragging(box.id);
                e.dataTransfer.effectAllowed = "move";
              }}
              onDragOver={(e) => {
                if (!dragging || dragging === box.id) return;
                e.preventDefault();
                const now = Date.now();
                if (now - lastSwap.current > 80) {
                  onReorder(dragging, box.id);
                  lastSwap.current = now;
                }
              }}
              onDragEnd={() => { setDragging(null); grabbed.current = false; }}
              onClose={() => onClose(box.id)}
              onResizeStart={(e) => startResize(box.id, e)}
            >
              {render(box.id)}
            </Tile>
          );
        })}
      </div>
    </div>
  );
}
