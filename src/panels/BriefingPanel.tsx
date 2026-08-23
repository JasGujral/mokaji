import { useCallback, useEffect, useState } from "react";
import { Panel } from "../components/Panel";
import { api } from "../lib/api";
import type { Briefing } from "../lib/types";

/** The morning briefing — **M-5**.
 *
 *  Every line is assembled Rust-side from records, with citations computed from what was supplied.
 *  Nothing here phrases anything: a briefing whose wording is generated in the renderer is one
 *  whose claims cannot be traced, and tracing is the whole argument. */
export function BriefingPanel({ tick }: { tick: number }) {
  const [b, setB] = useState<Briefing | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [speaking, setSpeaking] = useState(false);
  const [open, setOpen] = useState<number | null>(null);

  const load = useCallback(async () => {
    try { setB(await api.briefing()); setErr(null); } catch (e) { setErr(String(e)); }
  }, []);

  useEffect(() => { void load(); }, [load, tick]);

  const say = async () => {
    if (!b) return;
    if (speaking) { await api.hush().catch(() => {}); setSpeaking(false); return; }
    setSpeaking(true);
    try { await api.speak(`${b.greeting} ${b.spoken}`); } catch (e) { setErr(String(e)); }
    // `say` is fire-and-forget, so this is a best guess at duration rather than a callback.
    // Roughly 14 characters a second at the default rate; the button stays a stop button until
    // then, and pressing it early is the point.
    const secs = Math.min(120, Math.max(4, (b.spoken.length + b.greeting.length) / 14));
    setTimeout(() => setSpeaking(false), secs * 1000);
  };

  if (err) return <Panel title="Daily Briefing"><div className="empty">{err}</div></Panel>;
  if (!b) return <Panel title="Daily Briefing"><div className="empty">Assembling…</div></Panel>;

  return (
    <Panel title="Daily Briefing" sub={b.greeting}>
      <div className="brief">
        {b.lines.map((l, i) => (
          <div className="brief-line" key={`${l.section}-${i}`}>
            <span className="brief-sec">{l.section}</span>
            <span className="brief-txt">{l.text}</span>
            {l.citations.length > 0 && (
              <button
                className="brief-cite"
                title="What backs this"
                onClick={() => setOpen(open === i ? null : i)}
              >
                {l.citations.length}
              </button>
            )}
            {open === i && (
              <ul className="brief-refs">
                {l.citations.map((c) => (
                  <li key={c.record_id}>
                    <span className="src">{c.source}</span> {c.source_ref}
                  </li>
                ))}
              </ul>
            )}
          </div>
        ))}
      </div>

      <div className="brief-foot">
        <button className={`btn ${speaking ? "live" : ""}`} onClick={() => void say()}>
          {speaking ? "Stop" : "Read it out"}
        </button>
        {/* M-5's exit criterion, stated rather than implied. Two connectors and a hopeful
            sentence is not a three-connector briefing, and the panel should not pretend. */}
        <span className={b.three_connector ? "ok" : "muted"}>
          {b.sources.length === 0
            ? "no connector answered"
            : `${b.sources.length} of 3 senses · ${b.sources.join(" · ")}`}
        </span>
      </div>

      {b.failures.length > 0 && (
        <div className="brief-fail">
          {b.failures.map((f) => (
            <div key={f.connector}>
              <b>{f.connector}</b> {f.reason}
            </div>
          ))}
        </div>
      )}
    </Panel>
  );
}
