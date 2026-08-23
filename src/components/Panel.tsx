import type { ReactNode } from "react";

/** The glass shell every panel wears. Corner brackets come from CSS pseudo-elements, so a panel
 *  is one element and the Deck can move it without touching its contents. */
export function Panel({
  title, sub, style, children,
}: {
  title: string;
  sub?: ReactNode;
  style?: React.CSSProperties;
  children: ReactNode;
}) {
  return (
    <section className="panel" style={style} aria-label={title}>
      <header>
        <h3>{title}</h3>
        {sub ? <span className="sub">{sub}</span> : null}
      </header>
      <div className="content">{children}</div>
    </section>
  );
}

export function Bar({ pct, warn }: { pct: number; warn?: boolean }) {
  const clamped = Math.max(0, Math.min(100, pct));
  return (
    <div
      className={warn ? "bar warn" : "bar"}
      role="meter"
      aria-valuenow={clamped}
      aria-valuemin={0}
      aria-valuemax={100}
    >
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
