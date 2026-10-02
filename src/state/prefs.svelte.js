import { getAppConfig, updateAppConfig } from "../lib/api.js";
import { DEFAULT_HOTKEYS } from "../lib/hotkeys.js";
import { getLang } from "../lib/i18n/index.svelte.js";
import { readJson, readText, writeJson, writeText } from "../lib/storage.js";

const APP_KEY = "verdant.appPrefs";
const UPDATE_KEY = "verdant.updatePrefs";
const HOTKEYS_KEY = "verdant.hotkeys";
const SIDEBAR_KEY = "verdant.sidebarCollapsed";
const LIST_WIDTH_KEY = "verdant.listPaneWidth";

export const TEXT_SIZES = ["small", "medium", "large"];

const DEFAULT_APP = {
  runInBackground: true,
  autostart: false,
  showNotifications: true,
  notificationImportance: "all",
  textSize: "small",
  useDarkMode: false,
};

const DEFAULT_UPDATE = { autoCheck: true, autoDownload: false, channel: "stable" };

function normalizeChannel(value) {
  const channel = String(value || "").trim().toLowerCase();
  return channel === "nightly" || channel === "beta" ? "nightly" : "stable";
}

function loadApp() {
  const app = { ...DEFAULT_APP, ...readJson(APP_KEY, {}) };
  if (!TEXT_SIZES.includes(app.textSize)) app.textSize = "small";
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
  listPaneWidth: Number(readText(LIST_WIDTH_KEY)) || 0,
});

function backendConfig() {
  return {
    run_in_background: prefs.app.runInBackground,
    show_notifications: prefs.app.showNotifications !== false,
    notify_important_only: prefs.app.notificationImportance === "important",
    language: getLang(),
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
