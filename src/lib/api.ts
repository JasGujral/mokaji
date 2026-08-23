import { invoke } from "@tauri-apps/api/core";
import type { BootInfo, CalEvent, Chaser, Core, HealthRow, Metric, Preview, Task } from "./types";

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
  grammar: () => invoke<[string, string][]>("grammar"),
  setVault: (path: string) => invoke<string>("set_vault", { path }),
  setCalendar: (path: string) => invoke<string>("set_calendar", { path }),
  /** Booleans only — the renderer never receives a credential (PRIV-4). */
  secretStatus: () => invoke<Record<string, boolean>>("secret_status"),
  setSecret: (name: string, value: string) => invoke<void>("set_secret", { name, value }),
  clearSecret: (name: string) => invoke<void>("clear_secret", { name }),
};

/** Whether we are running inside the Tauri shell at all.
 *
 *  `npm run dev` in a plain browser has no backend, and a HUD that shows an empty deck with no
 *  explanation is indistinguishable from one whose vault is missing. Knowing which lets the UI say
 *  so. */
export const inTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
