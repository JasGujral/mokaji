import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { BootInfo, HealthRow, MailAccount } from "../lib/types";

export interface Appearance {
  hue: number;
  glow: number;
  wallpaper: "gradient" | "nebula" | "grid" | "plain";
  scanlines: boolean;
  noise: boolean;
}

export const DEFAULT_APPEARANCE: Appearance = {
  hue: 155, glow: 1, wallpaper: "gradient", scanlines: false, noise: true,
};

/** Accent hues offered by the design handoff. */
const HUES: [number, string][] = [
  [155, "green"], [195, "cyan"], [240, "ion blue"], [70, "amber"], [330, "magenta"],
];

/** Setup and configuration.
 *
 *  **Credentials are never displayed back.** The panel can tell you a key is *set* and let you
 *  replace or remove it; it cannot show it to you, because the renderer never receives one. Keys
 *  live in the macOS Keychain, Rust-side (PRIV-4), and the only thing that crosses the boundary is
 *  a boolean. */
export function Settings({
  boot, appearance, onAppearance, onVaultChanged, onClose,
}: {
  boot: BootInfo | null;
  appearance: Appearance;
  onAppearance: (a: Appearance) => void;
  onVaultChanged: () => void;
  onClose: () => void;
}) {
  const [vault, setVault] = useState(boot?.vault ?? "");
  const [vaultMsg, setVaultMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [cal, setCal] = useState(boot?.calendar ?? "");
  const [calMsg, setCalMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [health, setHealth] = useState<HealthRow[]>([]);
  const [secrets, setSecrets] = useState<Record<string, boolean>>({});
  const [key, setKey] = useState("");
  const [keyMsg, setKeyMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [calSuggestions, setCalSuggestions] = useState<string[]>([]);
  const [mail, setMail] = useState<MailAccount[]>([]);
  const [mailMsg, setMailMsg] = useState<Record<string, { ok: boolean; text: string }>>({});
  const [draft, setDraft] = useState<Record<string, { address: string; password: string }>>({});
  const [netOn, setNetOn] = useState(true);

  useEffect(() => { setVault(boot?.vault ?? ""); }, [boot?.vault]);
  useEffect(() => { setCal(boot?.calendar ?? ""); }, [boot?.calendar]);
  useEffect(() => {
    void api.health().then(setHealth).catch(() => setHealth([]));
    void api.secretStatus().then(setSecrets).catch(() => setSecrets({}));
    void api.suggestCalendars().then(setCalSuggestions).catch(() => setCalSuggestions([]));
    void api.mailAccounts().then(setMail).catch(() => setMail([]));
    void api.network().then((n) => setNetOn(n.allowed)).catch(() => setNetOn(true));
  }, [boot?.vault]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  async function saveVault() {
    try {
      const p = await api.setVault(vault);
      setVaultMsg({ ok: true, text: `Reading ${p}` });
      onVaultChanged();
      setHealth(await api.health());
    } catch (e) {
      setVaultMsg({ ok: false, text: String(e) });
    }
  }

  async function saveCal() {
    try {
      const p = await api.setCalendar(cal);
      setCalMsg({ ok: true, text: `Reading ${p}` });
      onVaultChanged();
      setHealth(await api.health());
    } catch (e) {
      setCalMsg({ ok: false, text: String(e) });
    }
  }

  async function saveKey() {
    try {
      await api.setSecret("anthropic", key);
      setKey("");
      setKeyMsg({ ok: true, text: "Stored in the macOS Keychain. It will not be shown again." });
      setSecrets(await api.secretStatus());
    } catch (e) {
      setKeyMsg({ ok: false, text: String(e) });
    }
  }

  async function clearKey() {
    await api.clearSecret("anthropic").catch(() => {});
    setKeyMsg({ ok: true, text: "Removed from the Keychain." });
    setSecrets(await api.secretStatus());
  }

  const slot = (name: "work" | "personal") => mail.find((m) => m.slot === name);
  const d = (name: string) => draft[name] ?? { address: "", password: "" };
  const edit = (name: string, patch: Partial<{ address: string; password: string }>) =>
    setDraft((p) => ({ ...p, [name]: { ...d(name), ...patch } }));

  async function saveMail(name: "work" | "personal") {
    const cur = slot(name);
    const address = (d(name).address || cur?.address || "").trim();
    const password = d(name).password.trim();
    if (!address) {
      setMailMsg((m) => ({ ...m, [name]: { ok: false, text: "an address is required" } }));
      return;
    }
    try {
      await api.setMailAccount({ slot: name, address, password: password || undefined });
      setDraft((p) => ({ ...p, [name]: { address: "", password: "" } }));
      setMail(await api.mailAccounts());
      setHealth(await api.health());
      onVaultChanged();
      setMailMsg((m) => ({
        ...m,
        [name]: {
          ok: true,
          text: password
            ? "Saved. The app password went to the Keychain and will not be shown again."
            : "Saved.",
        },
      }));
    } catch (e) {
      setMailMsg((m) => ({ ...m, [name]: { ok: false, text: String(e) } }));
    }
  }

  async function forgetMail(name: "work" | "personal") {
    await api.clearMailAccount(name).catch(() => {});
    setMail(await api.mailAccounts());
    setMailMsg((m) => ({ ...m, [name]: { ok: true, text: "Removed, Keychain item included." } }));
    onVaultChanged();
  }

  const mailbox = (name: "work" | "personal", label: string) => {
    const cur = slot(name);
    const msg = mailMsg[name];
    return (
      <div className="acct" key={name}>
        <div className="acct-head">
          <b>{label}</b>
          {cur ? (
            <span className={cur.has_password ? "ok" : "bad"}>
              {cur.has_password ? "app password set" : "no app password"}
            </span>
          ) : (
            <span className="muted">not configured</span>
          )}
        </div>
        <label htmlFor={`m-${name}-a`}>Address</label>
        <input id={`m-${name}-a`} className="field" spellCheck={false}
               placeholder={name === "work" ? "you@yourcompany.com" : "you@example.com"}
               value={d(name).address || cur?.address || ""}
               onChange={(e) => edit(name, { address: e.target.value })} />
        <label htmlFor={`m-${name}-p`}>App password</label>
        <input id={`m-${name}-p`} className="field" type="password" spellCheck={false}
               autoComplete="new-password"
               placeholder={cur?.has_password ? "•••• stored — type to replace" : "xxxx xxxx xxxx xxxx"}
               value={d(name).password}
               onChange={(e) => edit(name, { password: e.target.value })}
               onKeyDown={(e) => { if (e.key === "Enter") void saveMail(name); }} />
        <div className="row">
          <button className="btn" onClick={() => void saveMail(name)}>Save</button>
          {cur && <button className="btn" onClick={() => void forgetMail(name)}>Forget</button>}
          {msg && <span className={msg.ok ? "ok" : "bad"}>{msg.text}</span>}
        </div>
      </div>
    );
  };

  const set = (patch: Partial<Appearance>) => onAppearance({ ...appearance, ...patch });

  return (
    <div className="settings-scrim" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <aside className="settings" role="dialog" aria-label="Settings">
        <div className="row" style={{ justifyContent: "space-between", marginTop: 0 }}>
          <h2>Settings</h2>
          <button className="iconbtn" onClick={onClose} aria-label="Close settings">✕</button>
        </div>
        <p style={{ color: "var(--muted-2)" }}>
          {boot ? `v${boot.version} · ${boot.milestone}` : ""}
        </p>

        <h3>Vault</h3>
        <p>
          The folder MOKaji reads — an Obsidian vault containing <code>08 Journal/Daily</code>.
          A GUI app gets neither your shell environment nor a working directory, so this is the
          only way it can know. The choice is remembered in <code>~/.config/mokaji/vault</code>.
        </p>
        <label htmlFor="s-vault">Vault path</label>
        <input id="s-vault" className="field" value={vault} spellCheck={false}
               onChange={(e) => setVault(e.target.value)}
               onKeyDown={(e) => { if (e.key === "Enter") void saveVault(); }} />
        <div className="row">
          <button className="btn" onClick={() => void saveVault()}>Save</button>
          {vaultMsg && <span className={vaultMsg.ok ? "ok" : "bad"}>{vaultMsg.text}</span>}
        </div>

        <h3>Calendar</h3>
        <p>
          A folder of <code>.ics</code> files. No account and no OAuth — which is not a compromise
          so much as the shorter road: macOS Calendar already speaks to Google, and it writes every
          event of every account it syncs as its own <code>.ics</code> under{" "}
          <code>~/Library/Calendars</code>. Add your accounts in{" "}
          <b>System Settings → Internet Accounts</b>, point this at that folder, and both work and
          personal calendars are readable here with nothing leaving the machine.
        </p>
        <label htmlFor="s-cal">Calendar folder</label>
        <input id="s-cal" className="field" value={cal} spellCheck={false}
               onChange={(e) => setCal(e.target.value)}
               onKeyDown={(e) => { if (e.key === "Enter") void saveCal(); }} />
        {calSuggestions.length > 0 && (
          <div className="row">
            <span style={{ color: "var(--muted-2)" }}>found:</span>
            {calSuggestions.map((c) => (
              <button key={c} className="chip" onClick={() => setCal(c)}>{c}</button>
            ))}
          </div>
        )}
        <div className="row">
          <button className="btn" onClick={() => void saveCal()}>Save</button>
          {calMsg && <span className={calMsg.ok ? "ok" : "bad"}>{calMsg.text}</span>}
        </div>

        <h3>Voice</h3>
        <p>
          <b>⌥Space</b> anywhere on the machine opens the command bar, even when MOKaji is hidden;
          <b> ⌘K</b> does the same when the window already has focus. Everything the Console can do,
          it can do — add a task, tick one off, open a note in Obsidian, bring a panel forward, or
          send the window away and call it back. One parser serves both, so a command cannot behave
          differently typed and spoken.
        </p>
        <p>
          Dictation needs the local speech engine, which arrives with the wake word (<b>Hey Kaji</b>)
          at M-3; until then the command bar takes typing. Nothing spoken ever leaves this machine —
          the audio crate has no network dependency and cannot acquire one without a visible change
          that fails review and CI.
        </p>

        <h3>Mail</h3>
        <p>
          The third sense. <b>Read and classify only</b> — MOKaji cannot send, reply, archive,
          delete or mark anything read, and it opens your mailbox with <code>EXAMINE</code> rather
          than <code>SELECT</code>, so looking at your mail here leaves no trace in the client you
          actually read it in. Headers only; no message body is ever fetched.
        </p>
        <p>
          Use a <b>Google app password</b>, not your account password. Turn on 2-Step Verification,
          then <b>myaccount.google.com → Security → App passwords</b>, and paste the sixteen
          characters below — spaces and all, they get stripped. It goes straight to the macOS
          Keychain under its own service name, so revoking one account cannot touch the other.
        </p>
        {mailbox("work", "Work")}
        {mailbox("personal", "Personal")}

        <h3>Network</h3>
        <p>
          One switch for everything outbound (PRIV-5). Cutting it costs you the mail line and
          nothing else: the briefing is assembled from records by ordinary code with no model
          involved, so it stays correct with the cable out.
        </p>
        <div className="row">
          <button className="btn" onClick={() => void api.setNetwork(!netOn).then(setNetOn)}>
            {netOn ? "Cut outbound traffic" : "Restore outbound traffic"}
          </button>
          <span className={netOn ? "ok" : "bad"}>
            {netOn ? "outbound allowed" : "outbound cut"}
          </span>
        </div>

        <h3>Connectors</h3>
        {health.length === 0 ? (
          <p className="empty">nothing registered</p>
        ) : (
          health.map((h) => (
            <div className="kv" key={h.connector}>
              <span className="k">{h.connector}</span>
              <span className={h.state === "ok" ? "ok" : "bad"}>
                {h.state}{h.detail ? ` — ${h.detail}` : ""}
              </span>
            </div>
          ))
        )}
        <p style={{ color: "var(--muted-2)" }}>
          Each connector reports its own health (A-6): an expired work password degrades the mail
          rows, not the Deck.
        </p>

        <h3>Anthropic API key</h3>
        <p>
          Optional, and unused until M-4. Stored in the <strong>macOS Keychain</strong>, never on
          disk and never in this window — the renderer only ever learns whether one is set.
        </p>
        <div className="kv">
          <span className="k">status</span>
          <span className={secrets.anthropic ? "ok" : "bad"}>
            {secrets.anthropic ? "set" : "not set"}
          </span>
        </div>
        <label htmlFor="s-key">Key</label>
        <input id="s-key" className="field" type="password" value={key} autoComplete="off"
               placeholder={secrets.anthropic ? "replace the stored key" : "sk-ant-…"}
               onChange={(e) => setKey(e.target.value)}
               onKeyDown={(e) => { if (e.key === "Enter") void saveKey(); }} />
        <div className="row">
          <button className="btn" onClick={() => void saveKey()} disabled={!key.trim()}>Store</button>
          <button className="btn danger" onClick={() => void clearKey()} disabled={!secrets.anthropic}>
            Remove
          </button>
          {keyMsg && <span className={keyMsg.ok ? "ok" : "bad"}>{keyMsg.text}</span>}
        </div>

        <h3>Writing to the vault</h3>
        <p>
          <strong>Dry-run, and not changeable here yet.</strong> The Console shows exactly what a
          command would do and writes nothing. Arming the write path belongs with the voice loop's
          spoken confirmation and 30-second undo (M-2), not with a toggle in a settings panel that
          has neither.
        </p>

        <h3>Appearance</h3>
        <label>Accent</label>
        <div className="swatches">
          {HUES.map(([h, name]) => (
            <button key={h} title={name}
              className={appearance.hue === h ? "swatch on" : "swatch"}
              onClick={() => set({ hue: h })}
              aria-label={name}
              style={{ background: `oklch(0.78 0.17 ${h})`, color: `oklch(0.78 0.17 ${h})` }} />
          ))}
        </div>

        <label htmlFor="s-wall">Wallpaper</label>
        <select id="s-wall" className="field" value={appearance.wallpaper}
                onChange={(e) => set({ wallpaper: e.target.value as Appearance["wallpaper"] })}>
          <option value="gradient">gradient</option>
          <option value="nebula">nebula</option>
          <option value="grid">grid</option>
          <option value="plain">plain</option>
        </select>

        <label htmlFor="s-glow">Glow · {appearance.glow.toFixed(1)}</label>
        <input id="s-glow" type="range" min={0} max={2} step={0.1} value={appearance.glow}
               style={{ width: "100%" }}
               onChange={(e) => set({ glow: Number(e.target.value) })} />

        <div className="row">
          <label style={{ marginTop: 0 }}>
            <input type="checkbox" checked={appearance.scanlines}
                   onChange={(e) => set({ scanlines: e.target.checked })} /> scanlines
          </label>
          <label style={{ marginTop: 0 }}>
            <input type="checkbox" checked={appearance.noise}
                   onChange={(e) => set({ noise: e.target.checked })} /> film grain
          </label>
        </div>
        <p style={{ color: "var(--muted-2)" }}>
          Atmosphere is a preference; legibility is not. Every layer here can be turned off, and
          <code> prefers-reduced-motion</code> already disables the animation regardless.
        </p>
      </aside>
    </div>
  );
}
