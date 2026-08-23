import { invoke } from "@tauri-apps/api/core";
import type { Action, Applied, Briefing, MailAccount, BootInfo, CalEvent, Chaser, Core, HealthRow, Metric, Preview, Task } from "./types";

/** Every path to data goes through a Tauri command. The renderer holds no credential, opens no
 *  socket, and never touches the filesystem — SEC-1's allow-list is the entire surface. */

export const api = {
  bootInfo: () => invoke<BootInfo>("boot_info"),
  core: () => invoke<Core>("core"),
  tasks: () => invoke<Task[]>("tasks"),
  chasers: () => invoke<Chaser[]>("chasers"),
  agenda: () => invoke<CalEvent[]>("agenda"),
  vitals: () => invoke<Metric[]>("vitals"),
  health: () => invoke<HealthRow[]>("health"),
  preview: (input: string) => invoke<Preview>("preview", { input }),
  apply: (input: string) => invoke<Applied>("apply", { input }),
  undoWrite: (undoId: string) => invoke<string>("undo_write", { undoId }),
  grammar: () => invoke<[string, string][]>("grammar"),
  setVault: (path: string) => invoke<string>("set_vault", { path }),
  setCalendar: (path: string) => invoke<string>("set_calendar", { path }),
  /** Booleans only — the renderer never receives a credential (PRIV-4). */
  secretStatus: () => invoke<Record<string, boolean>>("secret_status"),
  setSecret: (name: string, value: string) => invoke<void>("set_secret", { name, value }),
  clearSecret: (name: string) => invoke<void>("clear_secret", { name }),
  /** CON-3: the one parser, reachable from the Console and the voice loop alike. Reports what an
   *  utterance means; deliberately does not act on it. */
  act: (input: string) => invoke<Action>("act", { input }),
  windowHide: () => invoke<void>("window_hide"),
  windowShow: () => invoke<void>("window_show"),
  openNote: (query: string) => invoke<string>("open_note", { query }),
  /** Folders that look like calendars — `~/Library/Calendars` is the zero-credential route to
   *  every account macOS already knows about. */
  suggestCalendars: () => invoke<string[]>("suggest_calendars"),

  /** M-5. Assembled locally, with citations computed from the records rather than requested from
   *  a model — a claim that cannot be traced is indistinguishable from a plausible invention. */
  briefing: () => invoke<Briefing>("briefing"),
  speak: (text: string) => invoke<void>("speak", { text }),
  hush: () => invoke<void>("hush"),

  /** Configured mailboxes. Never a password — only whether one is set. */
  mailAccounts: () => invoke<MailAccount[]>("mail_accounts"),
  setMailAccount: (a: {
    slot: string; address: string; password?: string; mailbox?: string; enabled?: boolean;
  }) => invoke<void>("set_mail_account", a),
  clearMailAccount: (slot: string) => invoke<void>("clear_mail_account", { slot }),

  /** PRIV-5's kill switch, exposed so "stop talking to the network" is one click rather than a
   *  setting you have to find. */
  network: () => invoke<{ allowed: boolean; recent: string[] }>("network"),
  setNetwork: (allowed: boolean) => invoke<boolean>("set_network", { allowed }),
};

/** Whether we are running inside the Tauri shell at all.
 *
 *  `npm run dev` in a plain browser has no backend, and a HUD that shows an empty deck with no
 *  explanation is indistinguishable from one whose vault is missing. Knowing which lets the UI say
 *  so. */
export const inTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
