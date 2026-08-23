import { Bar, KV, Panel } from "../components/Panel";
import { Ring } from "../components/Ring";
import type { Core } from "../lib/types";

/** The signature instrument. Philosophy from the handoff: **it reads you, not the machine.** */
export function CorePanel({ core, style }: { core: Core | null; style?: React.CSSProperties }) {
  if (!core) return <Panel title="Reactor Core" style={style}><div className="empty">reading…</div></Panel>;

  return (
    <Panel title="Reactor Core" sub={`${core.open} open`} style={style}>
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
