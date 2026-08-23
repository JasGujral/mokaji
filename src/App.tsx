import { useCallback, useEffect, useState } from "react";
import manifestJson from "./panels.json";
import { listen } from "@tauri-apps/api/event";
import { api, inTauri } from "./lib/api";
import type { BootInfo, CalEvent, Core, PanelManifest, Task } from "./lib/types";
import { Deck, type Size } from "./components/Deck";
import { Palette } from "./components/Palette";
import { TopBar } from "./components/TopBar";
import { Setup } from "./components/Setup";
import { Settings, DEFAULT_APPEARANCE, type Appearance } from "./components/Settings";
import { CorePanel } from "./panels/CorePanel";
import { BriefingPanel } from "./panels/BriefingPanel";
import { TasksPanel } from "./panels/TasksPanel";
import { AgendaPanel } from "./panels/AgendaPanel";
import { ConsolePanel } from "./panels/ConsolePanel";

const manifest = manifestJson as unknown as PanelManifest;
const DEFAULT_DECK = manifest.decks[0]?.panels ?? Object.keys(manifest.panels);

/** C-11: only UI state persists client-side. X-14 is explicit that canonical state lives in the
 *  connector sources — deck layout, sizes and appearance are the exception, because they are about
 *  the window rather than about your life. */
const UI_KEY = "mokaji.ui.v2";

interface Ui {
  visible: string[];
  sizes: Record<string, Size>;
  collapsed: boolean;
  appearance: Appearance;
}

function loadUi(): Ui {
  const fallback: Ui = {
    visible: DEFAULT_DECK, sizes: {}, collapsed: false, appearance: DEFAULT_APPEARANCE,
  };
  try {
    const raw = localStorage.getItem(UI_KEY);
    if (!raw) return fallback;
    const p = JSON.parse(raw) as Partial<Ui>;
    return {
      visible: Array.isArray(p.visible)
        ? p.visible.filter((id) => id in manifest.panels)
        : fallback.visible,
      sizes: p.sizes ?? {},
      collapsed: Boolean(p.collapsed),
      appearance: { ...DEFAULT_APPEARANCE, ...(p.appearance ?? {}) },
    };
  } catch {
    // A corrupt preference is not worth a broken app.
    return fallback;
  }
}

export default function App() {
  const [ui, setUi] = useState<Ui>(loadUi);
  const [core, setCore] = useState<Core | null>(null);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [events, setEvents] = useState<CalEvent[]>([]);
  const [boot, setBoot] = useState<BootInfo | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);

  useEffect(() => {
    try { localStorage.setItem(UI_KEY, JSON.stringify(ui)); } catch { /* private mode */ }
  }, [ui]);

  // Appearance is applied as CSS variables on the root, so every token that derives from --hue or
  // --glow follows without a single component knowing a theme exists.
  useEffect(() => {
    const r = document.documentElement;
    r.style.setProperty("--hue", String(ui.appearance.hue));
    r.style.setProperty("--glow", String(ui.appearance.glow));
  }, [ui.appearance.hue, ui.appearance.glow]);

  const refresh = useCallback(async () => {
    if (!inTauri()) {
      setErr("Not running inside the MOKaji window — `npm run dev` has no backend. Use `npm run tauri dev`.");
      return;
    }
    try {
      const [c, t, b, ev] = await Promise.all([
        api.core(), api.tasks(), api.bootInfo(), api.agenda().catch(() => [] as CalEvent[]),
      ]);
      setCore(c); setTasks(t); setBoot(b); setEvents(ev); setErr(null);
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    // B-6: the vault watcher pushes, so an edit in Obsidian lands here in about a second. The poll
    // stays as a backstop — a watcher that silently dies would otherwise leave the Deck frozen at
    // whatever it last read, which looks exactly like a calm day.
    const t = setInterval(() => void refresh(), 60_000);
    let stop: (() => void) | undefined;
    if (inTauri()) {
      void listen("vault-changed", () => void refresh()).then((un) => { stop = un; });
    }
    return () => { clearInterval(t); stop?.(); };
  }, [refresh]);

  const toggle = (id: string) =>
    setUi((u) => ({
      ...u,
      visible: u.visible.includes(id) ? u.visible.filter((x) => x !== id) : [...u.visible, id],
    }));

  const reorder = (dragged: string, target: string) =>
    setUi((u) => {
      const next = [...u.visible];
      const from = next.indexOf(dragged);
      const to = next.indexOf(target);
      if (from < 0 || to < 0) return u;
      next.splice(to, 0, ...next.splice(from, 1));
      return { ...u, visible: next };
    });

  const resize = (id: string, size: Size) =>
    setUi((u) => ({ ...u, sizes: { ...u.sizes, [id]: size } }));

  const render = (id: string) => {
    const spec = manifest.panels[id];
    if (!spec) return null;
    switch (spec.type) {
      case "core":     return <CorePanel core={core} />;
      case "briefing": return <BriefingPanel core={core} tasks={tasks} />;
      case "tasks":    return <TasksPanel tasks={tasks} />;
      case "agenda":   return <AgendaPanel events={events} hasCalendar={Boolean(boot?.calendar)} />;
      case "console":  return <ConsolePanel onWrote={() => void refresh()} />;
      // C-7: an unknown panel type is reported, never silently dropped — a typo in the manifest
      // must not look identical to a panel you forgot to enable.
      default:
        return <div className="empty">No renderer for panel type <code>{spec.type}</code>.</div>;
    }
  };

  const a = ui.appearance;
  const wallClass =
    a.wallpaper === "plain" ? "" : a.wallpaper === "gradient" ? "wallpaper" : `wallpaper ${a.wallpaper}`;

  return (
    <>
      {a.wallpaper !== "plain" && <div className={wallClass} />}
      <div className="vignette" />
      {a.scanlines && <div className="scanlines" />}
      {a.noise && <div className="noise" />}

      <div className="app">
        <TopBar
          core={core}
          boot={boot}
          onSettings={() => setSettingsOpen(true)}
          onTogglePalette={() => setUi((u) => ({ ...u, collapsed: !u.collapsed }))}
        />
        <div className="body">
          <Palette
            manifest={manifest}
            visible={ui.visible}
            toggle={toggle}
            collapsed={ui.collapsed}
          />
          {boot && boot.vault === null ? (
            <div className="deck" style={{ padding: 14 }}>
              <Setup onDone={() => void refresh()} />
            </div>
          ) : err ? (
            <div className="deck" style={{ padding: 14 }}>
              <div className="empty">{err}</div>
            </div>
          ) : (
            <Deck
              order={ui.visible}
              specs={manifest.panels}
              sizes={ui.sizes}
              onReorder={reorder}
              onResize={resize}
              onClose={toggle}
              render={render}
            />
          )}
        </div>
      </div>

      {settingsOpen && (
        <Settings
          boot={boot}
          appearance={ui.appearance}
          onAppearance={(ap) => setUi((u) => ({ ...u, appearance: ap }))}
          onVaultChanged={() => void refresh()}
          onClose={() => setSettingsOpen(false)}
        />
      )}
    </>
  );
}
