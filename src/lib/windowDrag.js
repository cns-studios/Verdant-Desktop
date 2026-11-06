import { getCurrentWindow } from "@tauri-apps/api/window";

const isInteractive = (event) =>
  event.target instanceof Element && !!event.target.closest("button, a, input, select, [role='button']");

export function startDrag(event) {
  if (event.button !== 0 || event.detail > 1 || isInteractive(event)) return;
  getCurrentWindow().startDragging().catch(() => {});
}

export function toggleMaximize(event) {
  if (!isInteractive(event)) getCurrentWindow().toggleMaximize().catch(() => {});
}
