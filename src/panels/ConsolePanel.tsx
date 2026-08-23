import { useState } from "react";
import { Panel } from "../components/Panel";
import { api } from "../lib/api";

type Line = { kind: "in" | "out" | "diff" | "note"; text: string };

/** The Console.
 *
 *  CON-1 — every line is parsed locally before any model is consulted. CON-3 — it is the *same*
 *  parser the voice loop uses, so a command cannot behave one way typed and another way spoken.
 *  CON-4 — a mutating command says what it will do before doing it, and shows the exact diff.
 *
 *  **B-4: the writer behind this is in dry-run.** Nothing typed here changes the vault. That is
 *  structural, not restraint — arming the write path belongs with the voice loop's spoken
 *  confirmation and 30-second undo, not with a text box that has neither. */
export function ConsolePanel({}: Record<string, never>) {
  const [log, setLog] = useState<Line[]>([
    { kind: "note", text: "MOKaji console — dry-run. Type a command to see exactly what it would do." },
    { kind: "note", text: "try: add a task to order the lamp oil tomorrow" },
  ]);
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    const cmd = value.trim();
    if (!cmd || busy) return;
    setValue("");
    setBusy(true);
    setLog((l) => [...l, { kind: "in", text: `> ${cmd}` }]);

    if (cmd === "clear") {
      setLog([]); setBusy(false); return;
    }

    try {
      if (cmd === "help") {
        const g = await api.grammar();
        setLog((l) => [
          ...l,
          ...g.map(([syntax, what]) => ({ kind: "out" as const, text: `  ${syntax}  —  ${what}` })),
        ]);
      } else {
        const p = await api.preview(cmd);
        setLog((l) => [
          ...l,
          { kind: "out", text: p.describes },
          ...(p.diff ? [{ kind: "diff" as const, text: p.diff.trimEnd() }] : []),
          ...(p.mutating
            ? [{ kind: "note" as const, text: "Dry-run — nothing was written. The armed write path lands with the voice loop, which has the spoken confirmation and 30-second undo this box does not." }]
            : []),
          ...(p.unmatched
            ? [{ kind: "note" as const, text: "No local command matched. Escalating to a model is M-4's job, and it will say so when it does — CON-2 makes that the caller's decision, not a silent one." }]
            : []),
        ]);
      }
    } catch (err) {
      setLog((l) => [...l, { kind: "note", text: String(err) }]);
    } finally {
      setBusy(false);
    }
  }

  const colour = (k: Line["kind"]) =>
    k === "in" ? "var(--neon-soft)" : k === "diff" ? "var(--muted)" : k === "note" ? "var(--muted-2)" : "var(--text)";

  return (
    <Panel title="Command Console" sub="dry-run">
      <div>
        {log.map((line, i) => (
          <div
            key={i}
            className="console-line"
            style={{
              color: colour(line.kind),
              whiteSpace: line.kind === "diff" ? "pre" : "pre-wrap",
              fontSize: line.kind === "diff" ? 10 : undefined,
            }}
          >
            {line.text}
          </div>
        ))}
      </div>
      <form onSubmit={submit}>
        <label htmlFor="console-in" style={{ position: "absolute", left: -9999 }}>Command</label>
        <input
          id="console-in"
          className="console-input"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          placeholder={busy ? "…" : "add a task to order the lamp oil tomorrow"}
          autoComplete="off"
          disabled={busy}
        />
      </form>
    </Panel>
  );
}
