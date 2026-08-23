import { useState } from "react";
import { Panel } from "./Panel";
import { api } from "../lib/api";

/** First-run: point MOKaji at a vault.
 *
 *  This exists because of a real failure. A macOS app launched from Finder inherits neither your
 *  shell environment nor a useful working directory, so `MOKAJI_VAULT_PATH` and the upward
 *  directory walk both fail for a double-clicked app — and with no data at all, the Reactor Core
 *  computed a perfectly honest **100% OPTIMAL**. Nothing to do and nothing to read produce the
 *  same arithmetic, and the healthiest possible reading is the worst possible way to say "I could
 *  not find your notes". */
export function Setup({ onDone }: { onDone: () => void }) {
  const [path, setPath] = useState("~/Documents/…/your-vault");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function save(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setErr(null);
    try {
      await api.setVault(path);
      onDone();
    } catch (e2) {
      setErr(String(e2));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Panel title="No vault yet" sub="first run">
      <p style={{ lineHeight: 1.7, margin: "0 0 12px" }}>
        MOKaji reads an Obsidian vault — a folder containing <code>08 Journal/Daily</code>.
        Point it at yours and the choice is remembered.
      </p>
      <form onSubmit={save}>
        <label htmlFor="vault-in" style={{ position: "absolute", left: -9999 }}>Vault path</label>
        <input
          id="vault-in"
          className="console-input"
          value={path}
          onChange={(ev) => setPath(ev.target.value)}
          spellCheck={false}
          autoComplete="off"
          disabled={busy}
        />
      </form>
      {err && <p style={{ color: "var(--warn)", marginTop: 10, lineHeight: 1.6 }}>{err}</p>}
      <p style={{ color: "var(--muted-2)", marginTop: 14, lineHeight: 1.6 }}>
        Press return to save. The terminal <code>mokaji</code> command finds the vault on its own
        because a shell has a working directory and an environment — an app launched from Finder
        has neither, which is the whole reason this panel exists.
      </p>
    </Panel>
  );
}
