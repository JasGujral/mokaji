import { useEffect, useState } from "react";
import type { BootInfo, Core } from "../lib/types";

export function TopBar({
  core, boot, onSettings, onTogglePalette,
}: {
  core: Core | null;
  boot: BootInfo | null;
  onSettings: () => void;
  onTogglePalette: () => void;
}) {
  const [now, setNow] = useState(new Date());
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(t);
  }, []);

  const hour = now.getHours();
  const greeting = hour < 5 ? "Still up" : hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening";
  const degraded = (core?.failures.length ?? 0) > 0;

  return (
    <header className="topbar">
      <button className="iconbtn" onClick={onTogglePalette} aria-label="Toggle panel list" title="Panels">
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.4">
          <path d="M1 3h12M1 7h12M1 11h12" />
        </svg>
      </button>
      <span className="greeting">{greeting}</span>
      <span className="meta">
        {now.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" })}
        {core ? ` · ${core.open} open · ${core.urgent} due today` : ""}
      </span>
      <span className="spacer" />
      {degraded && (
        <span className="pill warn" title={core?.failures.map((f) => `${f.connector}: ${f.reason}`).join("\n")}>
          {core?.failures.length} degraded
        </span>
      )}
      <span className="meta">{now.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}</span>
      <span className="pill">{boot ? `v${boot.version} · ${boot.milestone}` : "…"}</span>
      <button className="iconbtn" onClick={onSettings} aria-label="Settings" title="Settings">
        <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3">
          <circle cx="8" cy="8" r="2.4" />
          <path d="M8 1v2M8 13v2M1 8h2M13 8h2M3 3l1.5 1.5M11.5 11.5L13 13M13 3l-1.5 1.5M4.5 11.5L3 13" />
        </svg>
      </button>
    </header>
  );
}
