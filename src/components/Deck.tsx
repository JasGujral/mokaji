import { useEffect, useState } from "react";
import { columnsFor, pack } from "../lib/pack";
import type { PanelSpec } from "../lib/types";

/** The Deck. It knows how to place panels and nothing about what any of them contain — `render`
 *  is supplied by the caller, so adding a panel type never touches this file. */
export function Deck({
  order, specs, render,
}: {
  order: string[];
  specs: Record<string, PanelSpec>;
  render: (id: string, style: React.CSSProperties) => React.ReactNode;
}) {
  const [cols, setCols] = useState(() =>
    columnsFor(typeof window === "undefined" ? 1440 : window.innerWidth));

  useEffect(() => {
    const onResize = () => setCols(columnsFor(window.innerWidth));
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  const placed = pack(order, specs, cols);

  return (
    <main className="deck" style={{ ["--cols" as string]: cols }}>
      {placed.map((p) =>
        render(p.id, {
          gridColumn: `${p.col} / span ${p.w}`,
          gridRow: `${p.row} / span ${p.h}`,
        }),
      )}
    </main>
  );
}
