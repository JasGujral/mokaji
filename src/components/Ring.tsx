/** The Reactor Core's arc. Inline SVG — the design handoff ships no image assets, and a HUD that
 *  fetches a picture to draw a number would break REL-3 for no reason. */
export function Ring({ pct, label, state }: { pct: number; label: string; state: string }) {
  const r = 62;
  const c = 2 * Math.PI * r;
  const dash = (Math.max(0, Math.min(100, pct)) / 100) * c;
  const strained = state === "STRAINED";
  const stroke = strained ? "var(--warn)" : "var(--neon)";

  return (
    <svg viewBox="0 0 160 160" width="100%" height="auto" style={{ maxHeight: 190 }}
         role="img" aria-label={`${label}: ${pct} percent, ${state}`}>
      <circle cx="80" cy="80" r={r} fill="none" stroke="oklch(0.35 0.02 235 / 0.5)" strokeWidth="6" />
      <circle
        cx="80" cy="80" r={r} fill="none" stroke={stroke} strokeWidth="6" strokeLinecap="round"
        strokeDasharray={`${dash} ${c - dash}`} transform="rotate(-90 80 80)"
        style={{ transition: "stroke-dasharray var(--reflow)" }}
      />
      <circle cx="80" cy="80" r={r - 12} fill="none" stroke="var(--hair)" strokeWidth="1" />
      <text x="80" y="76" textAnchor="middle" className="readout"
            style={{ fontSize: 30, fill: stroke }}>{pct}%</text>
      <text x="80" y="96" textAnchor="middle"
            style={{ fontSize: 9, letterSpacing: "0.26em", fill: "var(--muted)",
                     fontFamily: "var(--font-display)" }}>{state}</text>
    </svg>
  );
}
