const COOLDOWN_MS = {
  compose: 350,
  composeMaximize: 200,
  refresh: 1800,
  settings: 1200,
  search: 250,
  send: 1000,
  switchNextAccount: 500,
};

export const DEFAULT_HOTKEYS = {
  enabled: true,
  compose: "ctrl+n",
  composeMaximize: "h",
  refresh: "ctrl+r",
  settings: "ctrl+,",
  search: "ctrl+k",
  send: "ctrl+enter",
  close: "escape",
  switchNextAccount: "ctrl+tab",
  nextMailbox: "<",
};

export const EDITABLE_HOTKEYS = [
  { key: "compose", label: "settings.shortcuts.compose" },
  { key: "composeMaximize", label: "settings.shortcuts.maximize" },
  { key: "refresh", label: "settings.shortcuts.refresh" },
  { key: "settings", label: "settings.shortcuts.settings" },
  { key: "search", label: "settings.shortcuts.search" },
  { key: "send", label: "settings.shortcuts.send" },
  { key: "switchNextAccount", label: "settings.shortcuts.switch_account" },
  { key: "nextMailbox", label: "settings.shortcuts.next_mailbox" },
];

const lastRunAt = new Map();

export function eventCombo(event) {
  if (event.key === "Escape") return "escape";
  const parts = [];
  if (event.ctrlKey) parts.push("ctrl");
  if (event.altKey) parts.push("alt");
  if (event.shiftKey) parts.push("shift");
  parts.push(event.key.toLowerCase());
  return parts.join("+").replace(/\s+/g, "").replace("control", "ctrl");
}

export function canRunHotkey(action) {
  const cooldown = COOLDOWN_MS[action] || 0;
  const now = Date.now();
  if (now - (lastRunAt.get(action) || 0) < cooldown) return false;
  lastRunAt.set(action, now);
  return true;
}

const KEY_LABELS = { ctrl: "Ctrl", alt: "Alt", shift: "Shift", meta: "Meta", enter: "Enter", escape: "Esc", tab: "Tab" };

export function formatCombo(combo) {
  if (!combo) return "-";
  return combo
    .split("+")
    .map((part) => KEY_LABELS[part] ?? (part.length === 1 ? part.toUpperCase() : part[0].toUpperCase() + part.slice(1)))
    .join(" + ");
}

export function isTypingTarget(target) {
  return target instanceof Element && !!target.closest("input, textarea, select, [contenteditable='true']");
}
