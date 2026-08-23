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
  /** False when no connector answered at all — "nothing to do" and "nothing was read" must never
   *  look alike, and the arithmetic alone cannot tell them apart. */
  has_data: boolean;
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

export interface CalEvent {
  id: string; title: string; start: string; end: string;
  all_day: boolean; location: string | null; soon: boolean;
  source: string; source_ref: string;
}

export interface Chaser {
  id: string; kind: string; what: string; since: string;
  overdue: boolean; source_ref: string;
}

export interface Metric { key: string; value: string; at: string; }
export interface HealthRow { connector: string; state: string; detail: string | null; }
/** What a Console command would do, without doing it (B-4, CON-4). */
export interface Preview {
  kind: string;
  describes: string;
  mutating: boolean;
  diff: string;
  unmatched: boolean;
}

/** The result of actually applying a command. */
export interface Applied {
  path: string;
  diff: string;
  undo_id: string;
  undo_seconds: number;
}

export interface BootInfo { vault: string | null; calendar: string | null; version: string; milestone: string; }

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

/** What the Rust side says an utterance means — CON-3: typed and spoken produce the same tag.
 *
 *  The union is exhaustive so a new intent the UI forgets to handle is a type error at the
 *  `switch`, not silence at the microphone. */
export type Action =
  | { kind: "write"; describe: string }
  | { kind: "panel"; name: string; on: boolean }
  | { kind: "open"; query: string }
  | { kind: "window"; on: boolean }
  | { kind: "ui"; name: string }
  | { kind: "brief" }
  | { kind: "hush" }
  | { kind: "unmatched"; text: string };

/** A pointer from a briefing claim back to the record that makes it true (E-8). */
export interface Citation { record_id: string; source: string; source_ref: string; }

/** One statement in the briefing, and its evidence. */
export interface BriefingLine { section: string; text: string; citations: Citation[]; }

/** The morning briefing. Assembled locally from records with no model involved — E-2 pins the
 *  daily loop off the network, and the strongest way to keep it there is to need nothing that
 *  could be off the network. */
export interface Briefing {
  greeting: string;
  lines: BriefingLine[];
  spoken: string;
  sources: string[];
  /** M-5's exit criterion, answered directly rather than implied. */
  three_connector: boolean;
  failures: { connector: string; reason: string }[];
}

/** One configured mailbox. `has_password` is a boolean because PRIV-4 means the renderer learns
 *  that a credential exists and nothing more. */
export interface MailAccount {
  slot: "work" | "personal";
  address: string;
  host: string;
  port: number;
  mailbox: string;
  enabled: boolean;
  has_password: boolean;
}
