import { Panel } from "../components/Panel";
import type { Core, Task } from "../lib/types";

/** A written summary rather than more numbers. The Core already shows the gauges; repeating them
 *  in prose would be decoration. This says what to do about them. */
export function BriefingPanel({
  core, tasks, style,
}: { core: Core | null; tasks: Task[]; style?: React.CSSProperties }) {
  const now = new Date();
  const date = now.toLocaleDateString(undefined, {
    weekday: "long", day: "numeric", month: "long",
  });

  let line = "Reading the vault…";
  if (core) {
    if (core.urgent > 0) {
      line = `${core.urgent} task${core.urgent === 1 ? "" : "s"} due today. Clear ${core.urgent === 1 ? "it" : "those"} first — everything else can wait.`;
    } else if (core.overdue > 0) {
      line = `Nothing due today, but ${core.overdue} chaser${core.overdue === 1 ? " is" : "s are"} overdue. A nudge costs a minute.`;
    } else if (core.open === 0) {
      line = "The queue is empty. That is allowed.";
    } else if (core.momentum === 0) {
      line = `${core.open} open, nothing cleared yet today. Pick one and finish it — momentum only counts today's completions.`;
    } else {
      line = `${core.done_today} cleared today, ${core.open} still open. Nothing is on fire.`;
    }
  }

  const next = tasks.filter((t) => t.urgent).slice(0, 3);
  const top = next.length > 0 ? next : tasks.slice(0, 3);

  return (
    <Panel title="Daily Briefing" sub={date} style={style}>
      <p style={{ margin: "0 0 12px", lineHeight: 1.7, color: "var(--text)" }}>{line}</p>
      {top.length > 0 && (
        <>
          <div style={{ color: "var(--muted-2)", fontSize: 10, letterSpacing: "0.2em",
                        textTransform: "uppercase", fontFamily: "var(--font-display)",
                        margin: "0 0 6px" }}>
            {next.length > 0 ? "Due today" : "Top of the queue"}
          </div>
          {top.map((t) => (
            <div className="task" key={t.id}>
              <span className={t.urgent ? "due urgent" : "due"}>
                {t.due ? new Date(t.due).toLocaleDateString() : "—"}
              </span>
              <span className="txt">{t.text}</span>
            </div>
          ))}
        </>
      )}
      <p style={{ marginTop: 14, color: "var(--muted-2)", lineHeight: 1.6 }}>
        Agenda and email arrive at M-5. Until then the briefing is vault-only, and says so.
      </p>
    </Panel>
  );
}
