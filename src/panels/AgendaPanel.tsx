import { Panel } from "../components/Panel";

/** Deliberately empty until M-5.
 *
 *  An empty panel that explains itself is honest; one that silently shows nothing is
 *  indistinguishable from a broken connector, and teaches you to distrust the whole Deck. */
export function AgendaPanel({}: Record<string, never>) {
  return (
    <Panel title="Today's Agenda" sub="no calendar connector">
      <div className="empty">
        No calendar is connected yet — that arrives at <strong>M-5</strong>, with Google Calendar
        and local <code>.ics</code> behind one Event model.
        <br /><br />
        Until then <code>calLoad</code> is 0 and <code>bandwidth</code> is computed with no events.
        That is documented behaviour, not a fault: the Reactor Core says so on its own face rather
        than quietly reporting a healthier day than you are having.
      </div>
    </Panel>
  );
}
