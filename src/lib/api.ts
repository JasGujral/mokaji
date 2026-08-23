import { invoke } from "@tauri-apps/api/core";
import type { BootInfo, Chaser, Core, HealthRow, Metric, Task } from "./types";

/** Every path to data goes through a Tauri command. The renderer holds no credential, opens no
 *  socket, and never touches the filesystem — SEC-1's allow-list is the entire surface. */

export const api = {
  bootInfo: () => invoke<BootInfo>("boot_info"),
  core: () => invoke<Core>("core"),
  tasks: () => invoke<Task[]>("tasks"),
  chasers: () => invoke<Chaser[]>("chasers"),
  vitals: () => invoke<Metric[]>("vitals"),
  health: () => invoke<HealthRow[]>("health"),
};

/** Whether we are running inside the Tauri shell at all.
 *
 *  `npm run dev` in a plain browser has no backend, and a HUD that shows an empty deck with no
 *  explanation is indistinguishable from one whose vault is missing. Knowing which lets the UI say
 *  so. */
export const inTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
