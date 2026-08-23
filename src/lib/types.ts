/** Mirrors the Rust views in `src-tauri/src/lib.rs`. Keep the two in step — the renderer never
 *  reaches past these into the vault, which is what keeps SEC-1's default-deny meaningful. */

export interface Failure { connector: string; reason: string; }

export interface Core {
  readiness: number;
  state: "OPTIMAL" | "STEADY" | "STRAINED";
  focus: number;
  momentum: number;
  bandwidth: number;
  cal_load: number;
  open: number;
  done_today: number;
  urgent: number;
  overdue: number;
  events: number;
  failures: Failure[];
}

export interface Task {
  id: string;
  text: string;
  done: boolean;
  due: string | null;
  urgent: boolean;
  project: string | null;
  tags: string[];
  source: string;
  source_ref: string;
}

export interface Chaser {
  id: string; kind: string; what: string; since: string;
  overdue: boolean; source_ref: string;
}

export interface Metric { key: string; value: string; at: string; }
export interface HealthRow { connector: string; state: string; detail: string | null; }
export interface BootInfo { vault: string | null; version: string; milestone: string; }

/** One entry in `panels.json`. */
export interface PanelSpec {
  name: string;
  group: string;
  type: string;
  grid_data: { w: number; h: number; min_w?: number };
  query?: { kind: string; window?: string };
  refresh?: { mode: string; fallback_secs?: number };
}

export interface PanelManifest {
  schema_version: number;
  panels: Record<string, PanelSpec>;
  decks: { id: string; name: string; panels: string[] }[];
}
