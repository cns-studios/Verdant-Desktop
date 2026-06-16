import { logout, clearLocalData, getMailboxCounts, authStatus, listAccounts, removeAccount } from "../api.js";
import { checkForUpdates, downloadLatestUpdate } from "../api.js";
import { escapeHtml } from "../lib/format.js";
import { showToast } from "../lib/toast.js";
import { getHotkeys, saveHotkeys, defaultHotkeys, normalizeCombo, eventCombo } from "../lib/hotkeys.js";
import { syncMailboxInBackground, lastSynced } from "../lib/sync.js";
import { t, getLang, setLang, getSupportedLanguages } from "../lib/i18n.js";
import { getVersion } from "@tauri-apps/api/app";

const UPDATE_PREFS_KEY = "verdant.updatePrefs";
const defaultUpdatePrefs = { autoCheck: true, autoDownload: false, channel: "stable" };
export let updatePrefs = loadUpdatePrefs();

const APP_PREFS_KEY = "verdant.appPrefs";
const defaultAppPrefs = { runInBackground: true, autostart: false, showNotifications: true, notificationImportance: "all" };
export let appPrefs = loadAppPrefs();

function normalizeUpdateChannel(value) {
  const normalized = String(value || "").trim().toLowerCase();
  return normalized === "nightly" || normalized === "beta" ? "nightly" : "stable";
}

function loadUpdatePrefs() {
  try {
    const raw = localStorage.getItem(UPDATE_PREFS_KEY);
    const parsed = raw ? { ...defaultUpdatePrefs, ...JSON.parse(raw) } : { ...defaultUpdatePrefs };
    parsed.channel = normalizeUpdateChannel(parsed.channel);
    return parsed;
  } catch {
    return { ...defaultUpdatePrefs };
  }
}

export function saveUpdatePrefs(next) {
  updatePrefs = { ...defaultUpdatePrefs, ...next };
  updatePrefs.channel = normalizeUpdateChannel(updatePrefs.channel);
  localStorage.setItem(UPDATE_PREFS_KEY, JSON.stringify(updatePrefs));

  import("@tauri-apps/api/core").then(({ invoke }) => {
    invoke("update_app_config", { config: { update_channel: updatePrefs.channel } })
      .catch(err => console.error("Failed to sync update config to Rust", err));
  });
}

function loadAppPrefs() {
  try {
    const raw = localStorage.getItem(APP_PREFS_KEY);
    return raw ? { ...defaultAppPrefs, ...JSON.parse(raw) } : { ...defaultAppPrefs };
  } catch {
    return { ...defaultAppPrefs };
  }
}

export function saveAppPrefs(next) {
  appPrefs = { ...defaultAppPrefs, ...next };
  localStorage.setItem(APP_PREFS_KEY, JSON.stringify(appPrefs));
  
  import("@tauri-apps/api/core").then(({ invoke }) => {
    invoke("update_app_config", { config: { run_in_background: appPrefs.runInBackground } })
      .catch(err => console.error("Failed to sync app config to Rust", err));
  });
}

export async function hydratePrefsFromBackend() {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const config = await invoke("get_app_config");
    if (!config || typeof config !== "object") return;

    if (typeof config.run_in_background === "boolean") {
      appPrefs = { ...appPrefs, runInBackground: config.run_in_background };
      localStorage.setItem(APP_PREFS_KEY, JSON.stringify(appPrefs));
    }

    if (typeof config.update_channel === "string") {
      updatePrefs = { ...updatePrefs, channel: normalizeUpdateChannel(config.update_channel) };
      localStorage.setItem(UPDATE_PREFS_KEY, JSON.stringify(updatePrefs));
    }
  } catch (err) {
    console.error("Failed to hydrate prefs from backend config", err);
  }
}

function formatCombo(combo) {
  if (!combo) return "";
  const parts = combo.split("+").map(p => {
    if (p === "ctrl") return "Ctrl";
    if (p === "alt") return "Alt";
    if (p === "shift") return "Shift";
    if (p === "meta") return "Meta";
    if (p === "enter") return "Enter";
    if (p === "escape") return "Esc";
    if (p === "tab") return "Tab";
    if (p.length === 1) return p.toUpperCase();
    return p.charAt(0).toUpperCase() + p.slice(1);
  });
  return parts.join(" + ");
}

function setUpdateStatus(message, isError = false) {
  const el = document.getElementById("settings-update-status");
  if (!el) return;
  el.textContent = message;
  el.style.color = isError ? "#8a3b3b" : "";
}

export async function checkForAppUpdates(options = {}) {
  const {
    silent = true,
    autoDownload = false,
    updateSettingsUi = false,
    channel = updatePrefs.channel,
  } = options;

  try {
    const info = await checkForUpdates(channel);
    const channelLabel = channel === "nightly" ? t("settings.advanced.channel.nightly") : t("settings.advanced.channel.stable");

    if (updateSettingsUi) {
      if (info.updateAvailable) {
        setUpdateStatus(t("settings.app.update_available_status", { channel: channelLabel, version: info.latestVersion }));
        const btn = document.getElementById("settings-check-update");
        if (btn) {
          btn.textContent = t("settings.app.download_update");
          btn.dataset.updateReady = "true";
        }
      } else {
        setUpdateStatus(t("settings.app.up_to_date", { channel: channelLabel, version: info.currentVersion }));
      }
    }

    if (!info.updateAvailable) {
      if (!silent) showToast(t("toast.no_update"));
      return info;
    }

    if (!silent) showToast(t("toast.update_available", { channel: channelLabel, version: info.latestVersion }));

    if (autoDownload) {
      const downloaded = await downloadLatestUpdate(channel);
      if (updateSettingsUi) setUpdateStatus(t("toast.update_downloaded", { file: downloaded.fileName }));
      showToast(t("toast.update_downloaded", { file: downloaded.fileName }));
    }

    return info;
  } catch (error) {
    if (updateSettingsUi) {
      setUpdateStatus(t("settings.app.check_failed"), true);
    }
    if (!silent) showToast(`${t("settings.app.check_failed")}: ${String(error)}`, "error");
    return null;
  }
}

export async function runAutomaticUpdateFlow() {
  if (!updatePrefs.autoCheck) return;
  await checkForAppUpdates({
    silent: true,
    autoDownload: updatePrefs.autoDownload,
    updateSettingsUi: false,
    channel: updatePrefs.channel,
  });
}

export function isSettingsOpen() {
  return !!document.getElementById("verdant-overlay");
}

export function closeOverlay(immediate = false) {
  const overlay = document.getElementById("verdant-overlay");
  if (!overlay) return;
  if (immediate) { overlay.remove(); return; }
  overlay.classList.remove("open");
  setTimeout(() => overlay.remove(), 180);
}

function buildAppTab(profile, accounts, counts, langs, currentLang, version) {
  const lastInboxSync = lastSynced.get("INBOX")
    ? new Date(lastSynced.get("INBOX")).toLocaleString()
    : t("settings.app.not_synced");

  const activeAccount = accounts.find(a => a.is_active);

  return `
    <section class="settings-pane active" data-pane="app">
      <div class="settings-section-label">${escapeHtml(t("settings.app.language"))}</div>
      <div class="settings-card">
        <div class="settings-row">
          <span>${escapeHtml(t("settings.app.language"))}</span>
          <select id="settings-lang-select">
            ${langs.map(l => `<option value="${l.code}" ${l.code === currentLang ? "selected" : ""}>${escapeHtml(l.label)}</option>`).join("")}
          </select>
        </div>
      </div>

      <div class="settings-section-label">${escapeHtml(t("settings.app.connected_inboxes"))}</div>
      <div class="settings-inbox-list" id="settings-inbox-list">
        ${accounts.map(acc => {
          const isActive = acc.is_active;
          const statusParts = [];
          if (isActive) {
            statusParts.push(`${t("settings.app.inbox_status", { unread: counts.inbox_unread, total: counts.inbox_total })}`);
            statusParts.push(`${t("settings.app.last_sync")}: ${lastInboxSync}`);
          }
          const hoverText = isActive ? statusParts.join(" · ") : "";
          return `
            <div class="settings-inbox-item" data-account-id="${acc.id}" ${hoverText ? `title="${escapeHtml(hoverText)}"` : ""}>
              <div class="settings-inbox-main">
                <strong>${escapeHtml(acc.display_name || acc.email)}</strong>
                <span class="settings-inbox-provider">${escapeHtml(acc.provider)}${isActive ? ` · ${escapeHtml(t("settings.app.inbox_status", { unread: counts.inbox_unread, total: counts.inbox_total }))}` : ""}</span>
              </div>
              <div class="settings-inbox-actions">
                <button class="verdant-btn settings-danger settings-inbox-remove" data-account-id="${acc.id}" data-account-email="${escapeHtml(acc.email)}">
                  ${escapeHtml(isActive ? t("settings.app.logout") : t("settings.app.remove_account"))}
                </button>
              </div>
            </div>
          `;
        }).join("")}
      </div>

      <div class="settings-section-label">${escapeHtml(t("settings.app.update"))}</div>
      <div class="settings-card">
        <div class="settings-info-row">
          <span>${escapeHtml(t("settings.app.installed_version"))}</span>
          <strong id="settings-installed-version">v${escapeHtml(version || t("app.version_unknown"))}</strong>
        </div>
        <div class="settings-info-row">
          <span>${escapeHtml(t("settings.app.update_status"))}</span>
          <strong id="settings-update-status">${escapeHtml(t("settings.app.update_not_checked"))}</strong>
        </div>
      </div>
      <div class="settings-actions">
        <button class="verdant-btn" id="settings-check-update">${escapeHtml(t("settings.app.check_update"))}</button>
      </div>
    </section>
  `;
}

function buildBehaviorTab() {
  return `
    <section class="settings-pane" data-pane="behavior">
      <div class="settings-section-label">${escapeHtml(t("settings.behavior.notifications"))}</div>
      <div class="settings-card">
        <label class="settings-switch">
          <input type="checkbox" id="app-show-notifications" ${appPrefs.showNotifications ? "checked" : ""}>
          ${escapeHtml(t("settings.behavior.notifications"))}
        </label>
        <div class="settings-radio-group" id="notification-importance-group" style="margin-top:6px;${appPrefs.showNotifications ? "" : "opacity:0.5;pointer-events:none;"}">
          <label class="settings-radio">
            <input type="radio" name="notification-importance" value="all" ${appPrefs.notificationImportance !== "important" ? "checked" : ""}>
            ${escapeHtml(t("settings.behavior.all_mail"))}
          </label>
          <label class="settings-radio">
            <input type="radio" name="notification-importance" value="important" ${appPrefs.notificationImportance === "important" ? "checked" : ""}>
            ${escapeHtml(t("settings.behavior.only_important"))}
          </label>
        </div>
      </div>

      <div class="settings-section-label">${escapeHtml(t("settings.behavior.start_on_login"))}</div>
      <div class="settings-card">
        <label class="settings-switch">
          <input type="checkbox" id="app-autostart" ${appPrefs.autostart ? "checked" : ""}>
          ${escapeHtml(t("settings.behavior.start_on_login"))}
        </label>
        <label class="settings-switch">
          <input type="checkbox" id="app-run-background" ${appPrefs.runInBackground ? "checked" : ""}>
          ${escapeHtml(t("settings.behavior.run_in_background"))}
        </label>
      </div>
    </section>
  `;
}

function buildAppearenceTab() {
  return `
    <section class="settings-pane" data-pane="appearence">
      <div class="settings-section-label">${escapeHtml(t("settings.appearance.title"))}</div>
      <div class="settings-card">
        <div class="settings-radio-group" id="colorscheme-group">
          <label class="settings-radio">
            <input type="radio" name="colorscheme" value="light" ${!appPrefs.useDarkMode ? "checked" : ""}>
            ${escapeHtml(t("settings.appearence.light"))}
          </label>
          <label class="settings-radio">
            <input type="radio" name="colorscheme" value="dark" ${appPrefs.useDarkMode ? "checked" : ""}>
            ${escapeHtml(t("settings.appearence.dark"))}
          </label>
        </div>
      </div>
    </section>
  `;
}

function buildShortcutsTab() {
  const hotkeys = getHotkeys();
  const shortcuts = [
    { key: "compose", i18n: "settings.shortcuts.compose" },
    { key: "composeMaximize", i18n: "settings.shortcuts.maximize" },
    { key: "refresh", i18n: "settings.shortcuts.refresh" },
    { key: "settings", i18n: "settings.shortcuts.settings" },
    { key: "search", i18n: "settings.shortcuts.search" },
    { key: "send", i18n: "settings.shortcuts.send" },
    { key: "switchNextAccount", i18n: "settings.shortcuts.switch_account" },
  ];

  return `
    <section class="settings-pane" data-pane="shortcuts">
      <label class="settings-switch" style="margin-bottom:10px;">
        <input type="checkbox" id="hk-enabled" ${hotkeys.enabled ? "checked" : ""}>
        ${escapeHtml(t("settings.shortcuts.enabled"))}
      </label>
      <div class="settings-shortcut-list">
        ${shortcuts.map(s => {
          const combo = hotkeys[s.key] || "";
          return `
            <div class="settings-shortcut-row" data-shortcut-key="${s.key}">
              <span class="settings-shortcut-label">${escapeHtml(t(s.i18n))}</span>
              <div class="settings-shortcut-controls">
                <span class="settings-shortcut-key" data-shortcut-display="${s.key}">${combo ? escapeHtml(formatCombo(combo)) : escapeHtml("-")}</span>
                <button class="verdant-btn settings-shortcut-edit" data-shortcut-key="${s.key}">${escapeHtml(t("settings.shortcuts.edit"))}</button>
                <button class="verdant-btn settings-shortcut-unset" data-shortcut-key="${s.key}" ${combo ? "" : "disabled"}>${escapeHtml(t("settings.shortcuts.unset"))}</button>
              </div>
            </div>
          `;
        }).join("")}
      </div>
    </section>
  `;
}

function buildAdvancedTab() {
  return `
    <section class="settings-pane" data-pane="advanced">
      <div class="settings-section-label">${escapeHtml(t("settings.advanced.update_channel"))}</div>
      <div class="settings-card">
        <div class="settings-row">
          <span>${escapeHtml(t("settings.advanced.update_channel"))}</span>
          <select id="update-channel">
            <option value="stable" ${updatePrefs.channel === "stable" ? "selected" : ""}>${escapeHtml(t("settings.advanced.channel.stable"))}</option>
            <option value="nightly" ${updatePrefs.channel === "nightly" ? "selected" : ""}>${escapeHtml(t("settings.advanced.channel.nightly"))}</option>
          </select>
        </div>
        <div class="settings-help" style="margin-top:8px;">${escapeHtml(t("settings.advanced.channel_info"))}</div>
      </div>

      <div class="settings-section-label">${escapeHtml(t("settings.advanced.sync_all"))}</div>
      <div class="settings-card">
        <div class="settings-help">${escapeHtml(t("settings.advanced.cache_info"))}</div>
      </div>
      <div class="settings-actions">
        <button class="verdant-btn" id="settings-sync-all">${escapeHtml(t("settings.advanced.sync_all"))}</button>
        <button class="verdant-btn settings-danger" id="settings-clear">${escapeHtml(t("settings.advanced.clear_cache"))}</button>
      </div>
    </section>
  `;
}

export async function openSettingsModal(profile, currentMailbox, onLogout, onSync) {
  let auth = { connected: true };
  let counts = { inbox_total: 0, inbox_unread: 0, starred_total: 0, sent_total: 0, drafts_total: 0, archive_total: 0, trash_total: 0 };
  let accounts = [];

  try {
    [auth, counts, accounts] = await Promise.all([authStatus(), getMailboxCounts(), listAccounts()]);
  } catch {}

  const langs = getSupportedLanguages();
  const currentLang = getLang();

  let version = "";
  try {
    version = await getVersion();
  } catch {}

  closeOverlay(true);
  const overlay = document.createElement("div");
  overlay.id = "verdant-overlay";
  overlay.className = "verdant-overlay";
  overlay.innerHTML = `
    <div class="verdant-panel">
      <div class="verdant-head">
        <h2>${escapeHtml(t("settings.title"))}</h2>
        <button class="verdant-close" aria-label="Close">×</button>
      </div>
      <div class="settings-grid">
        <div class="settings-tabs">
          <button class="settings-tab active" data-tab="app">${escapeHtml(t("settings.tab.app"))}</button>
          <button class="settings-tab" data-tab="behavior">${escapeHtml(t("settings.tab.behavior"))}</button>
          <button class="settings-tab" data-tab="appearence">${escapeHtml(t("settings.tab.appearence"))}</button>
          <button class="settings-tab" data-tab="shortcuts">${escapeHtml(t("settings.tab.shortcuts"))}</button>
          <button class="settings-tab" data-tab="advanced">${escapeHtml(t("settings.tab.advanced"))}</button>
        </div>
        ${buildAppTab(profile, accounts, counts, langs, currentLang, version)}
        ${buildBehaviorTab()}
        ${buildAppearenceTab()}
        ${buildShortcutsTab()}
        ${buildAdvancedTab()}
      </div>
    </div>
  `;

  overlay.querySelector(".verdant-close")?.addEventListener("click", () => closeOverlay());
  overlay.addEventListener("click", (e) => { if (e.target === overlay) closeOverlay(); });

  document.body.appendChild(overlay);
  requestAnimationFrame(() => overlay.classList.add("open"));

  const panel = overlay.querySelector(".verdant-panel");

  const tabs = Array.from(panel.querySelectorAll(".settings-tab"));
  const panes = Array.from(panel.querySelectorAll(".settings-pane"));
  tabs.forEach((tab) => {
    tab.addEventListener("click", () => {
      const target = tab.getAttribute("data-tab");
      tabs.forEach((t) => t.classList.toggle("active", t === tab));
      panes.forEach((pane) => pane.classList.toggle("active", pane.getAttribute("data-pane") === target));
    });
  });

  panel.querySelector("#settings-lang-select")?.addEventListener("change", (e) => {
    setLang(e.target.value);
    closeOverlay();
    openSettingsModal(profile, currentMailbox, onLogout, onSync);
  });

  panel.querySelector("#app-show-notifications")?.addEventListener("change", (e) => {
    const enabled = !!e.target?.checked;
    saveAppPrefs({ ...appPrefs, showNotifications: enabled });
    const group = panel.querySelector("#notification-importance-group");
    if (group) {
      group.style.opacity = enabled ? "" : "0.5";
      group.style.pointerEvents = enabled ? "" : "none";
    }
  });

  panel.querySelectorAll('input[name="notification-importance"]').forEach(radio => {
    radio.addEventListener("change", (e) => {
      if (e.target?.checked) {
        saveAppPrefs({ ...appPrefs, notificationImportance: e.target.value });
      }
    });
  });

  panel.querySelector("#app-autostart")?.addEventListener("change", async (e) => {
    const enabled = !!e.target?.checked;
    saveAppPrefs({ ...appPrefs, autostart: enabled });
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      if (enabled) await invoke("autostart_enable");
      else await invoke("autostart_disable");
    } catch (err) {
      console.error("Failed to toggle autostart", err);
      showToast(t("settings.app.check_failed"), "error");
    }
  });

  panel.querySelector("#app-run-background")?.addEventListener("change", (e) => {
    saveAppPrefs({ ...appPrefs, runInBackground: !!e.target?.checked });
  });

  let shortcutListening = null;
  panel.querySelectorAll(".settings-shortcut-edit").forEach(btn => {
    btn.addEventListener("click", () => {
      const key = btn.dataset.shortcutKey;
      const displayEl = panel.querySelector(`[data-shortcut-display="${key}"]`);
      const unsetBtn = panel.querySelector(`.settings-shortcut-unset[data-shortcut-key="${key}"]`);
      if (!displayEl) return;

      if (shortcutListening) {
        const prevDisplay = panel.querySelector(`[data-shortcut-display="${shortcutListening}"]`);
        if (prevDisplay) prevDisplay.classList.remove("listening");
        const prevCombo = getHotkeys()[shortcutListening] || "";
        if (prevDisplay && prevDisplay.classList.contains("listening")) {
          prevDisplay.textContent = prevCombo ? formatCombo(prevCombo) : "-";
        }
      }
      shortcutListening = key;
      displayEl.textContent = t("settings.shortcuts.listening");
      displayEl.classList.add("listening");

      const handler = (e) => {
        if (e.key === "Escape") {
          cleanup();
          return;
        }

        if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) return;

        e.preventDefault();
        e.stopPropagation();

        const combo = normalizeCombo(eventCombo(e));
        const h = getHotkeys();
        h[key] = combo;
        saveHotkeys(h);
        cleanup();
        displayEl.textContent = formatCombo(combo);
        displayEl.classList.remove("listening");
        showToast(t("toast.shortcuts_saved"));
        if (unsetBtn) unsetBtn.disabled = false;
      };

      const cleanup = () => {
        document.removeEventListener("keydown", handler, true);
        shortcutListening = null;
        const el = panel.querySelector(`[data-shortcut-display="${key}"]`);
        if (el) {
          const currentCombo = getHotkeys()[key] || "";
          if (el.classList.contains("listening")) {
            el.textContent = currentCombo ? formatCombo(currentCombo) : "-";
            el.classList.remove("listening");
          }
        }
      };

      document.addEventListener("keydown", handler, true);
    });
  });

  panel.querySelectorAll(".settings-shortcut-unset").forEach(btn => {
    btn.addEventListener("click", () => {
      const key = btn.dataset.shortcutKey;
      const displayEl = panel.querySelector(`[data-shortcut-display="${key}"]`);
      const h = getHotkeys();
      h[key] = "";
      saveHotkeys(h);
      if (displayEl) displayEl.textContent = "-";
      btn.disabled = true;
      showToast(t("toast.shortcuts_saved"));
    });
  });

  panel.querySelector("#hk-enabled")?.addEventListener("change", (e) => {
    const h = getHotkeys();
    h.enabled = !!e.target?.checked;
    saveHotkeys(h);
  });

  panel.querySelector("#update-channel")?.addEventListener("change", (e) => {
    const value = normalizeUpdateChannel(e.target?.value);
    saveUpdatePrefs({ ...updatePrefs, channel: value });
    const label = value === "nightly" ? t("settings.advanced.channel.nightly") : t("settings.advanced.channel.stable");
    showToast(t("settings.app.channel_set", { channel: label }));
    const btn = panel.querySelector("#settings-check-update");
    if (btn) { btn.textContent = t("settings.app.check_update"); btn.dataset.updateReady = ""; }
  });

  panel.querySelectorAll('input[name="colorscheme"]').forEach((radio) => {
    radio.addEventListener("change", (e) => {
      const isDarkMode = e.target.value === "dark";
      saveAppPrefs({ ...appPrefs, useDarkMode: isDarkMode });
      document.documentElement.classList.toggle("dark", isDarkMode);
    });
  });

  panel.querySelector("#settings-check-update")?.addEventListener("click", async () => {
    const btn = panel.querySelector("#settings-check-update");

    if (btn?.dataset.updateReady === "true") {
      if (btn) { btn.disabled = true; btn.textContent = t("settings.app.downloading"); }
      setUpdateStatus(t("settings.app.downloading"));
      try {
        const downloaded = await downloadLatestUpdate(updatePrefs.channel);
        setUpdateStatus(t("toast.update_downloaded", { file: downloaded.fileName }));
        showToast(t("toast.update_downloaded", { file: downloaded.fileName }));

        if (btn) btn.textContent = t("update.installing");
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("install_and_relaunch", { filePath: downloaded.filePath });

        if (btn) btn.textContent = t("update.restarting");
        await new Promise(r => setTimeout(r, 600));
        const { exit } = await import("@tauri-apps/plugin-process");
        await exit(0);
      } catch (error) {
        setUpdateStatus(t("settings.app.download_failed"), true);
        showToast(`${t("settings.app.download_failed")}: ${String(error)}`, "error");
        if (btn) { btn.disabled = false; btn.textContent = t("settings.app.download_update"); btn.dataset.updateReady = "true"; }
      }
      return;
    }

    setUpdateStatus(t("settings.app.checking"));
    if (btn) { btn.disabled = true; btn.textContent = t("settings.app.checking"); }
    await checkForAppUpdates({ silent: false, autoDownload: false, updateSettingsUi: true, channel: updatePrefs.channel });
    if (btn) btn.disabled = false;
  });

  let isSyncingAll = false;
  panel.querySelector("#settings-sync-all")?.addEventListener("click", async () => {
    if (isSyncingAll) return;
    isSyncingAll = true;
    showToast(t("toast.fetching"));
    try {
      const { syncEmails } = await import("../api.js");
      await syncEmails();
      showToast(t("toast.sync_complete"));
    } catch (err) {
      showToast(String(err), "error");
    } finally {
      isSyncingAll = false;
    }
  });

  panel.querySelector("#settings-clear")?.addEventListener("click", async () => {
    await clearLocalData();
    closeOverlay();
    showToast(t("toast.db_cleared"));
    await onSync();
  });

  panel.querySelectorAll(".settings-inbox-remove").forEach(btn => {
    btn.addEventListener("click", async () => {
      const accountId = parseInt(btn.dataset.accountId, 10);
      const email = btn.dataset.accountEmail;
      if (accountId === auth.active_account_id) {
        await logout();
        closeOverlay();
        onLogout();
      } else {
        try {
          await removeAccount(accountId);
          showToast(`${email} removed`);
          closeOverlay();
          openSettingsModal(profile, currentMailbox, onLogout, onSync);
        } catch (err) {
          showToast(String(err), "error");
        }
      }
    });
  });
}
