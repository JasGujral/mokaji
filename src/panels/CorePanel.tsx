import { Bar, KV, Panel } from "../components/Panel";
import { Ring } from "../components/Ring";
import type { Core } from "../lib/types";

/** The signature instrument. Philosophy from the handoff: **it reads you, not the machine.** */
export function CorePanel({ core }: { core: Core | null }) {
  if (!core) return <Panel title="Reactor Core"><div className="empty">reading…</div></Panel>;

  // "Nothing to do" and "nothing was read" produce identical arithmetic — an empty task list is
  // 100% momentum by the formula, which is correct for the first and a lie for the second. Refuse
  // to show a reading at all rather than show the healthiest possible one over no data.
  if (!core.has_data) {
    return (
      <Panel title="Reactor Core" sub="no reading">
        <div className="empty">
          <span className="badge">NO DATA</span>
          <p style={{ lineHeight: 1.7 }}>
            Nothing answered, so there is no readout. An empty vault and an unreachable one look
            the same to the arithmetic — both give 100% — so no number is shown rather than the
            most flattering one.
          </p>
          {core.failures.map((f) => (
            <div key={f.connector} style={{ color: "var(--warn)" }}>
              {f.connector}: {f.reason}
            </div>
          ))}
        </div>
      </Panel>
    );
  }

  return (
    <Panel title="Reactor Core" sub={`${core.open} open`}>
      <Ring pct={core.readiness} label="Readiness" state={core.state} />
      <div style={{ marginTop: 10, display: "grid", gap: 8 }}>
        <div>
          <KV k="Focus clarity" v={`${core.focus}%`} />
          <Bar pct={core.focus} warn={core.urgent > 0 || core.overdue > 0} />
        </div>
        <div>
          <KV k="Momentum" v={`${core.momentum}%  (${core.done_today}/${core.open + core.done_today} today)`} />
          <Bar pct={core.momentum} />
        </div>
        <div>
          <KV k="Bandwidth" v={`${core.bandwidth}%`} />
          <Bar pct={core.bandwidth} />
        </div>
        <div style={{ marginTop: 4 }}>
          <KV k="Urgent (due ≤ today)" v={core.urgent} warn={core.urgent > 0} />
          <KV k="Chasers overdue" v={core.overdue} warn={core.overdue > 0} />
          {/* Documented, not hidden: calLoad is 0 until the calendar connector lands at M-5. */}
          <KV k="Calendar load" v={`${core.cal_load}%  · no calendar until M-5`} />
        </div>
      </div>
    </Panel>
  );
}
