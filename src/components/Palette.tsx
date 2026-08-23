import type { PanelManifest } from "../lib/types";

/** Panels grouped exactly as the manifest declares. The Palette reads `panels.json` — it has no
 *  list of its own, which is what stops the two drifting apart when a panel is added. */
export function Palette({
  manifest, visible, toggle, collapsed,
}: {
  manifest: PanelManifest;
  visible: string[];
  toggle: (id: string) => void;
  collapsed: boolean;
}) {
  const groups: Record<string, string[]> = {};
  for (const [id, spec] of Object.entries(manifest.panels)) {
    (groups[spec.group] ??= []).push(id);
  }

  return (
    <nav className={collapsed ? "palette collapsed" : "palette"} aria-label="Panels">
      {Object.entries(groups).map(([group, ids]) => (
        <div key={group}>
          {!collapsed && <h4>{group}</h4>}
          {ids.map((id) => {
            const on = visible.includes(id);
            const name = manifest.panels[id]?.name ?? id;
            return (
              <button
                key={id}
                className={on ? "on" : ""}
                onClick={() => toggle(id)}
                aria-pressed={on}
                title={name}
              >
                {collapsed ? name.slice(0, 2).toUpperCase() : name}
              </button>
            );
          })}
        </div>
      ))}
    </nav>
  );
}
