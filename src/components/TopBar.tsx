import { useEffect, useState } from "react";
import type { BootInfo, Core } from "../lib/types";

export function TopBar({ core, boot }: { core: Core | null; boot: BootInfo | null }) {
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
    </header>
  );
}
