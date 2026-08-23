/** The Reactor Core's instrument.
 *
 *  Three concentric rings rotating at 9, 18 and 26 seconds and a 3.4s core pulse — the handoff's
 *  numbers, not invented ones. Rotations at coprime-ish periods never resync, so the movement
 *  never settles into a pattern your eye can predict and start ignoring.
 *
 *  Inline SVG: the handoff ships no image assets, and a HUD that fetches a picture to draw a
 *  number would break REL-3 for nothing. Everything animates in CSS, so `prefers-reduced-motion`
 *  turns it all off in one place. */
export function Ring({ pct, state }: { pct: number; state: string }) {
  const R = 74;
  const C = 2 * Math.PI * R;
  const clamped = Math.max(0, Math.min(100, pct));
  const dash = (clamped / 100) * C;
  const strained = state === "STRAINED";
  const stroke = strained ? "var(--warn)" : "var(--neon)";

  // Tick marks: 60 around the dial, every fifth one long. An instrument reads as an instrument
  // because it has a scale, not because it has a circle.
  const ticks = Array.from({ length: 60 }, (_, i) => {
    const a = (i / 60) * Math.PI * 2 - Math.PI / 2;
    const long = i % 5 === 0;
    const r1 = R + 9;
    const r2 = R + (long ? 17 : 13);
    return (
      <line
        key={i}
        x1={100 + Math.cos(a) * r1} y1={100 + Math.sin(a) * r1}
        x2={100 + Math.cos(a) * r2} y2={100 + Math.sin(a) * r2}
        stroke={i / 60 <= clamped / 100 ? stroke : "var(--hair)"}
        strokeWidth={long ? 1.4 : 0.8}
        opacity={i / 60 <= clamped / 100 ? 0.9 : 0.5}
      />
    );
  });

  return (
    <svg viewBox="0 0 200 200" className="core-ring" role="img"
         aria-label={`Readiness ${clamped} percent, ${state}`}>
      <defs>
        <radialGradient id="coreglow">
          <stop offset="0%" stopColor={stroke} stopOpacity="0.28" />
          <stop offset="70%" stopColor={stroke} stopOpacity="0.05" />
          <stop offset="100%" stopColor={stroke} stopOpacity="0" />
        </radialGradient>
      </defs>

      <circle cx="100" cy="100" r="70" fill="url(#coreglow)" className="core-pulse" />
      {ticks}

      {/* Outer track and the arc that fills to readiness. */}
      <circle cx="100" cy="100" r={R} fill="none" stroke="oklch(0.35 0.02 235 / 0.55)" strokeWidth="5" />
      <circle cx="100" cy="100" r={R} fill="none" stroke={stroke} strokeWidth="5" strokeLinecap="round"
              strokeDasharray={`${dash} ${C - dash}`} transform="rotate(-90 100 100)"
              style={{ transition: "stroke-dasharray var(--reflow)",
                       filter: `drop-shadow(0 0 calc(6px * var(--glow)) ${stroke})` }} />

      {/* Three rotating rings, each broken so the motion is visible. */}
      <g className="spin-a">
        <circle cx="100" cy="100" r="60" fill="none" stroke="var(--hair-strong)" strokeWidth="1"
                strokeDasharray="40 22 8 22" />
      </g>
      <g className="spin-b">
        <circle cx="100" cy="100" r="52" fill="none" stroke="var(--hair)" strokeWidth="1"
                strokeDasharray="14 10" />
      </g>
      <g className="spin-c">
        <circle cx="100" cy="100" r="44" fill="none" stroke="var(--hair-strong)" strokeWidth="0.8"
                strokeDasharray="60 30" />
      </g>

      <circle cx="100" cy="100" r="36" fill="oklch(0.12 0.02 230 / 0.55)" stroke="var(--hair)" strokeWidth="1" />

      <text x="100" y="97" textAnchor="middle" className="readout"
            style={{ fontSize: 34, fill: stroke, letterSpacing: "-0.02em" }}>{clamped}</text>
      <text x="100" y="113" textAnchor="middle"
            style={{ fontSize: 8, letterSpacing: "0.32em", fill: "var(--muted)",
                     fontFamily: "var(--font-display)" }}>{state}</text>
    </svg>
  );
}

/** A small sparkline. The handoff puts one on Focus; a number without a direction is a number you
 *  cannot act on. */
export function Spark({ points, warn }: { points: number[]; warn?: boolean }) {
  if (points.length < 2) return null;
  const max = Math.max(...points, 1);
  const min = Math.min(...points, 0);
  const span = Math.max(1, max - min);
  const d = points
    .map((p, i) => {
      const x = (i / (points.length - 1)) * 100;
      const y = 20 - ((p - min) / span) * 18;
      return `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  return (
    <svg viewBox="0 0 100 20" preserveAspectRatio="none" className="spark" aria-hidden="true">
      <path d={d} fill="none" strokeWidth="1.4"
            stroke={warn ? "var(--warn-dim)" : "var(--neon-dim)"} />
    </svg>
  );
}
