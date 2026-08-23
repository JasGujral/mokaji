import type { ReactNode } from "react";

/** Panel contents. The glass, brackets and title bar live on the Tile now — a panel that drew its
 *  own frame inside another frame is the double-border look the prototype avoids. `sub` still
 *  renders, because a panel's subtitle is about its data, not its chrome. */
export function Panel({
  title, sub, children,
}: {
  title?: string;
  sub?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div style={{ height: "100%", display: "flex", flexDirection: "column", minHeight: 0 }}>
      {sub ? (
        <div className="sub" style={{ color: "var(--muted-2)", fontSize: 10, marginBottom: 8 }}>
          {sub}
        </div>
      ) : null}
      <div style={{ flex: 1, minHeight: 0 }} aria-label={title}>{children}</div>
    </div>
  );
}

export function Bar({ pct, warn }: { pct: number; warn?: boolean }) {
  const clamped = Math.max(0, Math.min(100, pct));
  return (
    <div className={warn ? "bar warn" : "bar"} role="meter"
         aria-valuenow={clamped} aria-valuemin={0} aria-valuemax={100}>
      <i style={{ width: `${clamped}%` }} />
    </div>
  );
}

export function KV({ k, v, warn }: { k: string; v: ReactNode; warn?: boolean }) {
  return (
    <div className="kv">
      <span className="k">{k}</span>
      <span className="v" style={warn ? { color: "var(--warn)" } : undefined}>{v}</span>
    </div>
  );
}
