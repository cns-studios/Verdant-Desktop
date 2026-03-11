import { writable } from "svelte/store";

export type FilterChip = "all" | "unread" | "attachments" | "flagged";

export const composeOpen = writable(false);
export const activeFilter = writable<FilterChip>("all");
export const syncing = writable(false);
export const syncError = writable<string | null>(null);
export const notification = writable<{ text: string; type: "info" | "error" } | null>(null);

export function showNotification(text: string, type: "info" | "error" = "info") {
  notification.set({ text, type });
  setTimeout(() => notification.set(null), 4000);
}
