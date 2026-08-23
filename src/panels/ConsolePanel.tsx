import { useEffect, useRef, useState } from "react";
import { Panel } from "../components/Panel";
import { api } from "../lib/api";

type Line = { kind: "in" | "out" | "diff" | "note" | "warn"; text: string };

/** The Console.
 *
 *  CON-1 — parsed locally before any model is consulted. CON-3 — the *same* parser the voice loop
 *  uses, so a command cannot behave one way typed and another way spoken. CON-4 — a mutating
 *  command states what it will do, waits for you to say yes, and is undoable for thirty seconds.
 *
 *  The two-step is the whole safety mechanism, and it is why there is no global "arm writes"
 *  switch: a preview goes through a writer that structurally cannot change a file, and applying is
 *  a separate action you take after reading the diff. A toggle would add risk without adding
 *  capability. */
export function ConsolePanel({ onWrote }: { onWrote: () => void }) {
  const [log, setLog] = useState<Line[]>([
    { kind: "note", text: "MOKaji console. Type a command to see exactly what it would do." },
    { kind: "note", text: "try: add a task to order the lamp oil tomorrow" },
  ]);
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState<string | null>(null);
  const [undo, setUndo] = useState<{ id: string; left: number } | null>(null);
  const endRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => { endRef.current?.scrollIntoView({ block: "end" }); }, [log]);

  // The countdown is not decoration: CON-4's window is the safety net for a mis-transcription, and
  // a net you cannot see the edge of is one you will not reach for in time.
  useEffect(() => {
    if (!undo) return;
    if (undo.left <= 0) { setUndo(null); return; }
    const t = setTimeout(() => setUndo({ ...undo, left: undo.left - 1 }), 1000);
    return () => clearTimeout(t);
  }, [undo]);

  const say = (...lines: Line[]) => setLog((l) => [...l, ...lines]);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    const cmd = value.trim();
    if (!cmd || busy) return;
    setValue("");
    setBusy(true);
    say({ kind: "in", text: `> ${cmd}` });

    if (cmd === "clear") { setLog([]); setPending(null); setBusy(false); return; }

    try {
      if (cmd === "help") {
        const g = await api.grammar();
        say(...g.map(([syntax, what]) => ({ kind: "out" as const, text: `  ${syntax}  —  ${what}` })));
      } else {
        const p = await api.preview(cmd);
        say({ kind: "out", text: p.describes });
        if (p.diff) say({ kind: "diff", text: p.diff.trimEnd() });
        if (p.mutating) {
          setPending(cmd);
          say({ kind: "note", text: "Nothing written yet. Press Apply, or ⌘↩, to write it." });
        } else if (p.unmatched) {
          setPending(null);
          say({ kind: "note", text: "No local command matched. Escalating to a model is M-4's job, and it will say so when it does." });
        } else {
          setPending(null);
        }
      }
    } catch (err) {
      say({ kind: "warn", text: String(err) });
    } finally {
      setBusy(false);
    }
  }

  async function applyPending() {
    if (!pending || busy) return;
    setBusy(true);
    try {
      const r = await api.apply(pending);
      say({ kind: "out", text: `Written to ${r.path}.` });
      setUndo({ id: r.undo_id, left: r.undo_seconds });
      setPending(null);
      onWrote();
    } catch (err) {
      // B-3 drift and B-5 snapshot failures land here. Both mean "we did not write", and both are
      // worth reading rather than dismissing.
      say({ kind: "warn", text: String(err) });
    } finally {
      setBusy(false);
    }
  }

  async function undoLast() {
    if (!undo) return;
    try {
      const msg = await api.undoWrite(undo.id);
      say({ kind: "out", text: msg });
      setUndo(null);
      onWrote();
    } catch (err) {
      say({ kind: "warn", text: String(err) });
    }
  }

  const colour = (k: Line["kind"]) =>
    k === "in" ? "var(--neon-soft)"
    : k === "diff" ? "var(--muted)"
    : k === "warn" ? "var(--warn)"
    : k === "note" ? "var(--muted-2)"
    : "var(--text)";

  return (
    <Panel title="Command Console" sub={pending ? "awaiting confirmation" : "ready"}>
      <div style={{ height: "100%", display: "flex", flexDirection: "column", minHeight: 0 }}>
        <div style={{ flex: 1, minHeight: 0, overflow: "auto" }}>
          {log.map((line, i) => (
            <div key={i} className="console-line"
                 style={{ color: colour(line.kind),
                          whiteSpace: line.kind === "diff" ? "pre" : "pre-wrap",
                          fontSize: line.kind === "diff" ? 10 : undefined }}>
              {line.text}
            </div>
          ))}
          <div ref={endRef} />
        </div>

        {(pending || undo) && (
          <div className="row" style={{ marginTop: 8 }}>
            {pending && (
              <button className="btn" onClick={() => void applyPending()} disabled={busy}>
                Apply
              </button>
            )}
            {undo && (
              <button className="btn danger" onClick={() => void undoLast()}>
                Undo · {undo.left}s
              </button>
            )}
          </div>
        )}

        <form onSubmit={submit}>
          <label htmlFor="console-in" style={{ position: "absolute", left: -9999 }}>Command</label>
          <input
            id="console-in"
            className="console-input"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && pending) {
                e.preventDefault();
                void applyPending();
              }
            }}
            placeholder={busy ? "…" : "add a task to order the lamp oil tomorrow"}
            autoComplete="off"
            disabled={busy}
          />
        </form>
      </div>
    </Panel>
  );
}
