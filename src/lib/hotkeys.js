const HOTKEY_COOLDOWN_MS = {
  compose: 350,
  composeMaximize: 200,
  refresh: 1800,
  settings: 1200,
  search: 250,
  send: 1000,
};

export const defaultHotkeys = {
  enabled: true,
  compose: "ctrl+n",
  composeMaximize: "h",
  refresh: "ctrl+r",
  settings: "ctrl+,",
  search: "ctrl+k",
  send: "ctrl+enter",
  close: "escape",
};

const lastHotkeyAt = new Map();

export function loadHotkeys() {
  try {
    const raw = localStorage.getItem("verdant.hotkeys");
    return raw ? { ...defaultHotkeys, ...JSON.parse(raw) } : { ...defaultHotkeys };
  } catch {
    return { ...defaultHotkeys };
  }
}

export function saveHotkeys(next) {
  localStorage.setItem("verdant.hotkeys", JSON.stringify(next));
}

export function normalizeCombo(input) {
  return (input || "")
    .toLowerCase()
    .replace(/\s+/g, "")
    .replace("control", "ctrl");
}

export function eventCombo(event) {
  if (event.key === "Escape") return "escape";
  const key = event.key.length === 1 ? event.key.toLowerCase() : event.key.toLowerCase();
  const parts = [];
  if (event.ctrlKey) parts.push("ctrl");
  if (event.altKey) parts.push("alt");
  if (event.shiftKey) parts.push("shift");
  parts.push(key);
  return parts.join("+");
}

export function canRunHotkey(action) {
  const cooldown = HOTKEY_COOLDOWN_MS[action] || 0;
  if (cooldown <= 0) return true;

  const now = Date.now();
  const last = lastHotkeyAt.get(action) || 0;
  if (now - last < cooldown) return false;

  lastHotkeyAt.set(action, now);
  return true;
}
