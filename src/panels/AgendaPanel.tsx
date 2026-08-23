import { Panel } from "../components/Panel";
import type { CalEvent } from "../lib/types";

const time = (iso: string) =>
  new Date(iso).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

/** Today's events, from `.ics` files.
 *
 *  The `.ics` route landed before Google Calendar deliberately: it needs no OAuth client, no app
 *  verification and no browser round-trip, so `calLoad` stops being permanently zero without
 *  waiting on anyone's API console. Every calendar application can export or subscribe to one. */
export function AgendaPanel({ events, hasCalendar }: { events: CalEvent[]; hasCalendar: boolean }) {
  if (!hasCalendar) {
    return (
      <Panel title="Today's Agenda" sub="no calendar folder">
        <div className="empty">
          Point MOKaji at a folder of <code>.ics</code> files in <strong>Settings</strong> and this
          fills in — no account, no OAuth, no browser round-trip. Most calendar apps can export or
          subscribe to one.
          <br /><br />
          Until then <code>calLoad</code> is 0 and <code>bandwidth</code> is computed with no
          events. Documented behaviour, not a fault — the Core says so on its own face rather than
          quietly reporting a healthier day than you are having.
        </div>
      </Panel>
    );
  }

  if (events.length === 0) {
    return (
      <Panel title="Today's Agenda" sub="clear">
        <div className="empty">Nothing scheduled today.</div>
      </Panel>
    );
  }

  return (
    <Panel title="Today's Agenda" sub={`${events.length} event${events.length === 1 ? "" : "s"}`}>
      {events.map((e) => (
        <div className="task" key={e.id}>
          <span className={e.soon ? "due urgent" : "due"}>
            {e.all_day ? "all day" : time(e.start)}
          </span>
          <span className="txt">
            {e.title}
            {e.soon ? <span className="badge" style={{ marginLeft: 8 }}>soon</span> : null}
            {e.location ? <><br /><span className="ref">{e.location}</span></> : null}
            <br />
            <span className="ref">{e.source_ref}</span>
          </span>
        </div>
      ))}
    </Panel>
  );
}
