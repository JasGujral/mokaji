import { Panel } from "../components/Panel";
import type { Task } from "../lib/types";

/** The queue, in the router's deterministic order (A-5): due ascending, nulls last, then text.
 *  Every row carries its `source_ref`, because a task you cannot trace back to a line in a file is
 *  a task you cannot trust. */
export function TasksPanel({ tasks }: { tasks: Task[] }) {
  return (
    <Panel title="Task Queue" sub={`${tasks.length} open`}>
      {tasks.length === 0 ? (
        <div className="empty">
          Nothing open. Either the queue is clear, or the vault has no <code>- [ ]</code> lines in
          <code> 01 Projects</code> or <code>08 Journal/Daily</code>.
        </div>
      ) : (
        tasks.map((t) => (
          <div className="task" key={t.id}>
            <span className={t.urgent ? "due urgent" : "due"}>
              {t.due
                ? new Date(t.due).toLocaleDateString(undefined, { day: "2-digit", month: "short" })
                : "  —  "}
            </span>
            <span className="txt">
              {t.text}
              {t.project ? <span className="ref"> · {t.project}</span> : null}
              <br />
              <span className="ref">{t.source_ref}</span>
            </span>
          </div>
        ))
      )}
    </Panel>
  );
}
