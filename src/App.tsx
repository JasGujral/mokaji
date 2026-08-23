import { useCallback, useEffect, useState } from "react";
import manifestJson from "./panels.json";
import { api, inTauri } from "./lib/api";
import type { BootInfo, Core, PanelManifest, Task } from "./lib/types";
import { Deck } from "./components/Deck";
import { Palette } from "./components/Palette";
import { TopBar } from "./components/TopBar";
import { Panel } from "./components/Panel";
import { CorePanel } from "./panels/CorePanel";
import { BriefingPanel } from "./panels/BriefingPanel";
import { TasksPanel } from "./panels/TasksPanel";
import { AgendaPanel } from "./panels/AgendaPanel";
import { ConsolePanel } from "./panels/ConsolePanel";

const manifest = manifestJson as unknown as PanelManifest;
const DEFAULT_DECK = manifest.decks[0]?.panels ?? Object.keys(manifest.panels);

/** C-11: only UI state persists client-side. X-14 is explicit that canonical state lives in the
 *  connector sources — deck layout and prefs are the exception, because they are about the window,
 *  not about your life. */
const UI_KEY = "mokaji.ui.v1";

function loadUi(): { visible: string[]; collapsed: boolean } {
  try {
    const raw = localStorage.getItem(UI_KEY);
    if (raw) {
      const p = JSON.parse(raw) as { visible?: string[]; collapsed?: boolean };
      return {
        visible: Array.isArray(p.visible) ? p.visible.filter((id) => id in manifest.panels) : DEFAULT_DECK,
        collapsed: Boolean(p.collapsed),
      };
    }
  } catch {
    // A corrupt or unreadable preference is not worth a broken app.
  }
  return { visible: DEFAULT_DECK, collapsed: false };
}

export default function App() {
  const [ui, setUi] = useState(loadUi);
  const [core, setCore] = useState<Core | null>(null);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [boot, setBoot] = useState<BootInfo | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    try { localStorage.setItem(UI_KEY, JSON.stringify(ui)); } catch { /* private mode */ }
  }, [ui]);

  const refresh = useCallback(async () => {
    if (!inTauri()) {
      setErr("Not running inside the MOKaji window — `npm run dev` has no backend. Use `npm run tauri dev`.");
      return;
    }
    try {
      const [c, t, b] = await Promise.all([api.core(), api.tasks(), api.bootInfo()]);
      setCore(c); setTasks(t); setBoot(b); setErr(null);
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    // Poll rather than watch, for now. B-6's filesystem watcher is an S requirement and lands with
    // the write path; a 60s poll is honest until then.
    const t = setInterval(() => void refresh(), 60_000);
    return () => clearInterval(t);
  }, [refresh]);

  const toggle = (id: string) =>
    setUi((u) => ({
      ...u,
      visible: u.visible.includes(id) ? u.visible.filter((x) => x !== id) : [...u.visible, id],
    }));

  const render = (id: string, style: React.CSSProperties) => {
    const spec = manifest.panels[id];
    if (!spec) return null;
    switch (spec.type) {
      case "core":     return <CorePanel key={id} core={core} style={style} />;
      case "briefing": return <BriefingPanel key={id} core={core} tasks={tasks} style={style} />;
      case "tasks":    return <TasksPanel key={id} tasks={tasks} style={style} />;
      case "agenda":   return <AgendaPanel key={id} style={style} />;
      case "console":  return <ConsolePanel key={id} style={style} />;
      // C-7: an unknown panel type is reported, never silently dropped — otherwise a typo in the
      // manifest looks exactly like a panel you forgot to enable.
      default:
        return (
          <Panel key={id} title={spec.name} style={style}>
            <div className="empty">No renderer for panel type <code>{spec.type}</code>.</div>
          </Panel>
        );
    }
  };

  return (
    <div className="app">
      <TopBar core={core} boot={boot} />
      <div className="body">
        <Palette
          manifest={manifest}
          visible={ui.visible}
          toggle={toggle}
          collapsed={ui.collapsed}
        />
        {err ? (
          <main className="deck" style={{ ["--cols" as string]: 1 }}>
            <Panel title="Not reading">
              <div className="empty">
                {err}
                {boot?.vault ? <><br /><br />Vault: <code>{boot.vault}</code></> : null}
              </div>
            </Panel>
          </main>
        ) : (
          <Deck order={ui.visible} specs={manifest.panels} render={render} />
        )}
      </div>
    </div>
  );
}
