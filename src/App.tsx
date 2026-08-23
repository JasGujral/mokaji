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
import { Voice } from "./components/Voice";

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
  const [voiceOpen, setVoiceOpen] = useState(false);
  /** Bumped on every refresh. Panels that fetch their own data (the briefing assembles four
   *  queries Rust-side) watch this rather than being handed props, so a vault edit reaches them
   *  through the same B-6 path as everything else. */
  const [beat, setBeat] = useState(0);

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
      setBeat((n) => n + 1);
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
    let stopSummon: (() => void) | undefined;
    if (inTauri()) {
      void listen("vault-changed", () => void refresh()).then((un) => { stop = un; });
      // V-1's floor: the OS-wide ⌥Space registered in Rust. A wake word that has not been trained
      // yet is not a path, and an always-on HUD you have to go and click is just an app.
      void listen("summon", () => setVoiceOpen(true)).then((un) => { stopSummon = un; });
    }
    return () => { clearInterval(t); stop?.(); stopSummon?.(); };
  }, [refresh]);

  // The same overlay from inside the window, for when it already has focus. ⌥Space is handled by
  // the OS-level shortcut and never reaches here.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setVoiceOpen((v) => !v);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  /** Resolve a spoken panel name to a manifest id.
   *
   *  The parser already restricted this to a closed list, so the job here is only to map the
   *  words a person says onto the ids the manifest happens to use — "briefing" is the panel called
   *  "Daily Briefing" whose id is `focus`, and nobody should have to know that. */
  const panelId = (spoken: string): string | undefined => {
    const n = spoken.trim().toLowerCase();
    if (n in manifest.panels) return n;
    const byName = Object.entries(manifest.panels).find(([, s]) => s.name.toLowerCase() === n);
    if (byName) return byName[0];
    const alias: Record<string, string> = {
      "reactor core": "core",
      briefing: "focus",
      "daily briefing": "focus",
      "task queue": "tasks",
      "today's agenda": "agenda",
      "command console": "console",
    };
    return alias[n];
  };

  const setPanel = (spoken: string, on: boolean) =>
    setUi((u) => {
      const id = panelId(spoken);
      if (!id) return u;
      const has = u.visible.includes(id);
      if (on === has) return u;
      return { ...u, visible: on ? [...u.visible, id] : u.visible.filter((x) => x !== id) };
    });

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
      case "core":     return <CorePanel core={core} events={events} />;
      case "briefing": return <BriefingPanel tick={beat} />;
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

      <Voice
        open={voiceOpen}
        onClose={() => setVoiceOpen(false)}
        handlers={{
          panel: setPanel,
          ui: (name) => {
            if (name === "status") setPanel("core", true);
            if (name === "help") setPanel("console", true);
          },
          wrote: () => void refresh(),
        }}
      />

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
