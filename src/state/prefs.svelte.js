import { getAppConfig, setAppIcon, updateAppConfig } from "../lib/api.js";
import { APP_ICONS, DEFAULT_APP_ICON } from "../lib/appIcons.js";
import { DEFAULT_HOTKEYS } from "../lib/hotkeys.js";
import { getLang } from "../lib/i18n/index.svelte.js";
import { readJson, readText, writeJson, writeText } from "../lib/storage.js";

const APP_KEY = "verdant.appPrefs";
const UPDATE_KEY = "verdant.updatePrefs";
const HOTKEYS_KEY = "verdant.hotkeys";
const SIDEBAR_KEY = "verdant.sidebarCollapsed";
const SIDEBAR_WIDTH_KEY = "verdant.sidebarWidth";
const LIST_WIDTH_KEY = "verdant.listPaneWidth";

export const THEMES = ["light", "dark", "midnight"];
export const COLORSCHEMES = ["green", "blue", "red", "orange", "yellow"];
export const TEXT_SIZES = ["small", "medium", "large"];
export const TEXT_ZOOM = { small: 1, medium: 1.1, large: 1.2 };

const DEFAULT_APP = {
  runInBackground: true,
  autostart: false,
  showNotifications: true,
  notificationImportance: "all",
  textSize: "small",
  theme: "light",
  useDarkMode: false,
  discordRpc: false,
  loadImages: true,
  showWhatsNew: true,
};

const DEFAULT_UPDATE = { autoCheck: true, autoDownload: false, channel: "stable" };

function normalizeChannel(value) {
  const channel = String(value || "").trim().toLowerCase();
  return channel === "nightly" || channel === "beta" ? "nightly" : "stable";
}

function loadApp() {
  const stored = readJson(APP_KEY, {});
  const app = { ...DEFAULT_APP, ...stored };
  if (!THEMES.includes(stored.theme)) app.theme = stored.useDarkMode === true ? "dark" : "light";
  if (!COLORSCHEMES.includes(app.color)) app.color = "green";
  if (!TEXT_SIZES.includes(app.textSize)) app.textSize = "small";
  if (!APP_ICONS.some((icon) => icon.id === app.appIcon)) app.appIcon = DEFAULT_APP_ICON;
  return app;
}

function loadUpdate() {
  const update = { ...DEFAULT_UPDATE, ...readJson(UPDATE_KEY, {}) };
  update.channel = normalizeChannel(update.channel);
  return update;
}

export const prefs = $state({
  app: loadApp(),
  update: loadUpdate(),
  hotkeys: { ...DEFAULT_HOTKEYS, ...readJson(HOTKEYS_KEY, {}) },
  sidebarCollapsed: readText(SIDEBAR_KEY) === "1",
  sidebarWidth: Number(readText(SIDEBAR_WIDTH_KEY)) || 0,
  listPaneWidth: Number(readText(LIST_WIDTH_KEY)) || 0,
});

function backendConfig() {
  return {
    run_in_background: prefs.app.runInBackground,
    show_notifications: prefs.app.showNotifications !== false,
    notify_important_only: prefs.app.notificationImportance === "important",
    language: getLang(),
    theme: prefs.app.theme,
    color: prefs.app.color,
    update_channel: prefs.update.channel,
  };
}

function pushToBackend(config) {
  updateAppConfig(config).catch((error) => console.error("Failed to sync app config", error));
}

export function saveAppPrefs(patch) {
  Object.assign(prefs.app, patch);
  writeJson(APP_KEY, prefs.app);
  pushToBackend(backendConfig());
}

export function saveAppIcon(id) {
  if (!APP_ICONS.some((icon) => icon.id === id) || id === prefs.app.appIcon) return;
  const previous = prefs.app.appIcon;
  Object.assign(prefs.app, { appIcon: id });
  writeJson(APP_KEY, prefs.app);
  setAppIcon(id).catch((error) => {
    console.error("Failed to set app icon", error);
    Object.assign(prefs.app, { appIcon: previous });
    writeJson(APP_KEY, prefs.app);
  });
}

export function saveUpdatePrefs(patch) {
  Object.assign(prefs.update, patch);
  prefs.update.channel = normalizeChannel(prefs.update.channel);
  writeJson(UPDATE_KEY, prefs.update);
  pushToBackend({ update_channel: prefs.update.channel });
}

export function saveHotkeys(patch) {
  Object.assign(prefs.hotkeys, patch);
  writeJson(HOTKEYS_KEY, prefs.hotkeys);
}

export function setSidebarCollapsed(collapsed) {
  prefs.sidebarCollapsed = collapsed;
  writeText(SIDEBAR_KEY, collapsed ? "1" : "0");
  pushToBackend({ sidebar_collapsed: collapsed });
}

export function setSidebarWidth(width) {
  prefs.sidebarWidth = width;
  writeText(SIDEBAR_WIDTH_KEY, String(width));
}

export function setListPaneWidth(width) {
  prefs.listPaneWidth = width;
  writeText(LIST_WIDTH_KEY, String(width));
}

export async function syncPrefsWithBackend() {
  try {
    const config = await getAppConfig();
    if (typeof config?.run_in_background === "boolean") {
      prefs.app.runInBackground = config.run_in_background;
      writeJson(APP_KEY, prefs.app);
    }
    if (APP_ICONS.some((icon) => icon.id === config?.app_icon)) {
      prefs.app.appIcon = config.app_icon;
      writeJson(APP_KEY, prefs.app);
    }
    if (typeof config?.update_channel === "string") {
      prefs.update.channel = normalizeChannel(config.update_channel);
      writeJson(UPDATE_KEY, prefs.update);
    }
    if (typeof config?.sidebar_collapsed === "boolean") {
      prefs.sidebarCollapsed = config.sidebar_collapsed;
      writeText(SIDEBAR_KEY, config.sidebar_collapsed ? "1" : "0");
    }
  } catch (error) {
    console.error("Failed to read app config", error);
  }
  pushToBackend(backendConfig());
}
