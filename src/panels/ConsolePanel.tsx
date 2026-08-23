import { useState } from "react";
import { Panel } from "../components/Panel";

/** The Console shell. The intent pipeline it will drive is shared with the voice loop (CON-3) —
 *  typed and spoken commands must not diverge, so the parser lands once, at M-2, and both use it.
 *  Until then this accepts input and says plainly what it cannot yet do. */
export function ConsolePanel({ style }: { style?: React.CSSProperties }) {
  const [log, setLog] = useState<string[]>([
    "MOKaji console — M-1. Read-only.",
    "The intent pipeline lands at M-2, shared with the voice loop so typed and spoken commands cannot drift apart.",
  ]);
  const [value, setValue] = useState("");

  function submit(e: React.FormEvent) {
    e.preventDefault();
    const cmd = value.trim();
    if (!cmd) return;
    setLog((l) => [
      ...l,
      `> ${cmd}`,
      "Not yet — writes need the hash guard (B-3), dry-run default (B-4) and session snapshot (B-5) first. Declaring the capability before those exist would be a lie the router would act on.",
    ]);
    setValue("");
  }

  return (
    <Panel title="Command Console" sub="M-2" style={style}>
      <div>
        {log.map((line, i) => (
          <div className="console-line" key={i}>{line}</div>
        ))}
      </div>
      <form onSubmit={submit}>
        <label htmlFor="console-in" style={{ position: "absolute", left: -9999 }}>Command</label>
        <input
          id="console-in"
          className="console-input"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          placeholder="add a task to call the accountant tomorrow"
          autoComplete="off"
        />
      </form>
    </Panel>
  );
}
