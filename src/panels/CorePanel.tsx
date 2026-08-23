import { Ring, Spark } from "../components/Ring";
import type { CalEvent, Core } from "../lib/types";

/** The Reactor Core — the signature instrument.
 *
 *  The handoff's philosophy: **it reads you, not the machine.** So the ring is surrounded by four
 *  corner readouts and a footer that changes with readiness, rather than a list of bars. Each
 *  corner carries a number *and* the thing that number is about — "84%" tells you nothing;
 *  "84% · 1 urgent in queue" tells you what to do next. */
export function CorePanel({ core, events }: { core: Core | null; events: CalEvent[] }) {
  if (!core) return <div className="empty">reading…</div>;

  // "Nothing to do" and "nothing was read" produce identical arithmetic — an empty task list is
  // 100% momentum by the formula, correct for the first and a lie for the second. Refuse to show a
  // reading at all rather than the healthiest possible one over no data.
  if (!core.has_data) {
    return (
      <div className="empty">
        <span className="badge">NO DATA</span>
        <p style={{ lineHeight: 1.7 }}>
          Nothing answered, so there is no readout. An empty vault and an unreachable one look the
          same to the arithmetic — both give 100% — so no number is shown rather than the most
          flattering one.
        </p>
        {core.failures.map((f) => (
          <div key={f.connector} style={{ color: "var(--warn)" }}>{f.connector}: {f.reason}</div>
        ))}
      </div>
    );
  }

  const next = events
    .filter((e) => !e.all_day && new Date(e.start).getTime() > Date.now())
    .sort((a, b) => a.start.localeCompare(b.start))[0];
  const nextTime = next
    ? new Date(next.start).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })
    : null;

  const footer =
    core.urgent > 0
      ? `${core.urgent} due today. Clear ${core.urgent === 1 ? "it" : "those"} first.`
      : core.overdue > 0
        ? `${core.overdue} chaser${core.overdue === 1 ? "" : "s"} overdue. A nudge costs a minute.`
        : core.open === 0
          ? "Queue empty. That is allowed."
          : core.momentum === 0
            ? `${core.open} open, none cleared yet. Pick one and finish it.`
            : `${core.done_today} cleared, ${core.open} open. Nothing is on fire.`;

  return (
    <div className="core">
      <div className="core-grid">
        <Readout
          className="tl" label="Focus clarity" value={`${core.focus}%`}
          note={core.urgent > 0 ? `${core.urgent} urgent in queue` : "nothing urgent"}
          warn={core.urgent > 0}
        >
          <Spark points={[core.focus, core.focus, core.focus]} warn={core.urgent > 0} />
        </Readout>

        <Readout
          className="tr" label="Momentum" value={`${core.momentum}%`}
          note={`${core.done_today}/${core.open + core.done_today} cleared today`}
        />

        <div className="core-center">
          <Ring pct={core.readiness} state={core.state} />
        </div>

        <Readout
          className="bl" label="Calendar" value={String(core.events)}
          note={nextTime ? `next ${nextTime}` : core.events === 0 ? "nothing scheduled" : "all day"}
        />

        <Readout
          className="br" label="Chasing" value={String(core.overdue)}
          note={core.overdue > 0 ? `${core.overdue} overdue` : "none overdue"}
          warn={core.overdue > 0}
        />
      </div>

      <div className="core-footer">
        <span className="core-bandwidth">
          bandwidth {core.bandwidth}%
          <i style={{ width: `${core.bandwidth}%` }} />
        </span>
        <span>{footer}</span>
      </div>
    </div>
  );
}

function Readout({
  className, label, value, note, warn, children,
}: {
  className: string;
  label: string;
  value: string;
  note: string;
  warn?: boolean;
  children?: React.ReactNode;
}) {
  return (
    <div className={`core-readout ${className}`}>
      <span className="core-label">{label}</span>
      <span className="core-value" style={warn ? { color: "var(--warn)" } : undefined}>{value}</span>
      <span className="core-note">{note}</span>
      {children}
    </div>
  );
}
