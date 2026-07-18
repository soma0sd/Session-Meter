// Shared Svelte stores for the frontend windows.
import { writable } from "svelte/store";
import type { UsageSnapshot, Settings, SessionStatus } from "./ipc";

export const usage = writable<UsageSnapshot | null>(null);
export const settings = writable<Settings | null>(null);
export const session = writable<SessionStatus>({
  logged_in: false,
  org_name: "",
  email: "",
});
