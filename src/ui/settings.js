import { logout, clearLocalData, getMailboxCounts, authStatus } from "../api.js";
import { checkForUpdates, downloadLatestUpdate } from "../api.js";
import { escapeHtml } from "../lib/format.js";
import { showToast } from "../lib/toast.js";
import { loadHotkeys, saveHotkeys, defaultHotkeys, normalizeCombo } from "../lib/hotkeys.js";
import { syncMailboxInBackground, lastSynced } from "../lib/sync.js";
import { getVersion } from "@tauri-apps/api/app";

const UPDATE_PREFS_KEY = "verdant.updatePrefs";
const defaultUpdatePrefs = { autoCheck: true, autoDownload: false, channel: "stable" };
export let updatePrefs = loadUpdatePrefs();

function loadUpdatePrefs() {
  try {
    const raw = localStorage.getItem(UPDATE_PREFS_KEY);
    const parsed = raw ? { ...defaultUpdatePrefs, ...JSON.parse(raw) } : { ...defaultUpdatePrefs };
    parsed.channel = parsed.channel === "nightly" ? "nightly" : "stable";
    return parsed;
  } catch {
    return { ...defaultUpdatePrefs };
  }
}

export function saveUpdatePrefs(next) {
  updatePrefs = { ...defaultUpdatePrefs, ...next };
  updatePrefs.channel = updatePrefs.channel === "nightly" ? "nightly" : "stable";
  localStorage.setItem(UPDATE_PREFS_KEY, JSON.stringify(updatePrefs));
}

function setSettingsUpdateStatus(message, isError = false) {
  const statusEl = document.getElementById("settings-update-status");
  if (!statusEl) return;
  statusEl.textContent = message;
  statusEl.style.color = isError ? "#8a3b3b" : "";
}

function setSettingsUpdateDownloadButtonEnabled(enabled) {
  const button = document.getElementById("settings-download-update");
  if (!button) return;
  button.disabled = !enabled;
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

    if (updateSettingsUi) {
      if (info.updateAvailable) {
        const channelLabel = channel === "nightly" ? "Nightly" : "Stable";
        setSettingsUpdateStatus(`${channelLabel} update available: v${info.latestVersion}`);
      } else {
        setSettingsUpdateStatus(`Up to date on ${channel} (v${info.currentVersion})`);
      }
      setSettingsUpdateDownloadButtonEnabled(!!info.updateAvailable);
    }

    if (!info.updateAvailable) {
      if (!silent) showToast("You are on the latest version");
      return info;
    }

    if (!silent) showToast(`${channel} update v${info.latestVersion} available`);

    if (autoDownload) {
      const downloaded = await downloadLatestUpdate(channel);
      if (updateSettingsUi) setSettingsUpdateStatus(`Downloaded ${downloaded.fileName}`);
      showToast(`Update downloaded: ${downloaded.fileName}`);
    }

    return info;
  } catch (error) {
    if (updateSettingsUi) {
      setSettingsUpdateStatus("Update check failed", true);
      setSettingsUpdateDownloadButtonEnabled(false);
    }
    if (!silent) showToast(`Update check failed: ${String(error)}`, "error");
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

let hotkeys = loadHotkeys();

export function isSettingsOpen() {
  return !!document.getElementById("verdant-overlay");
}

function showOverlay(title, message, buttons) {
  closeOverlay(true);
  const overlay = document.createElement("div");
  overlay.id = "verdant-overlay";
  overlay.className = "verdant-overlay";
  overlay.innerHTML = `
    <div class="verdant-panel">
      <div class="verdant-head">
        <h2>${escapeHtml(title)}</h2>
        <button class="verdant-close" aria-label="Close">x</button>
      </div>
      <p>${escapeHtml(message)}</p>
      <div class="verdant-actions"></div>
    </div>
  `;

  overlay.querySelector(".verdant-close")?.addEventListener("click", () => closeOverlay());
  overlay.addEventListener("click", (event) => { if (event.target === overlay) closeOverlay(); });

  const actions = overlay.querySelector(".verdant-actions");
  for (const btn of buttons) {
    const el = document.createElement("button");
    el.className = `verdant-btn ${btn.primary ? "primary" : ""}`;
    el.textContent = btn.label;
    el.onclick = btn.onClick;
    actions.appendChild(el);
  }

  document.body.appendChild(overlay);
  requestAnimationFrame(() => overlay.classList.add("open"));
}

export function closeOverlay(immediate = false) {
  const overlay = document.getElementById("verdant-overlay");
  if (!overlay) return;
  if (immediate) { overlay.remove(); return; }
  overlay.classList.remove("open");
  setTimeout(() => overlay.remove(), 180);
}

export async function openSettingsModal(profile, currentMailbox, onLogout, onSync) {
  let auth = { connected: true };
  let counts = { inbox_total: 0, inbox_unread: 0, starred_total: 0, sent_total: 0, drafts_total: 0, archive_total: 0 };

  try {
    [auth, counts] = await Promise.all([authStatus(), getMailboxCounts()]);
  } catch (error) {
    console.warn("Failed to load extended settings details", error);
  }

  const lastInboxSync = lastSynced.get("INBOX")
    ? new Date(lastSynced.get("INBOX")).toLocaleString()
    : "Not synced in this session";

  showOverlay("Settings", `Signed in as ${profile.email}`, []);
  const panel = document.querySelector("#verdant-overlay .verdant-panel");
  if (!panel) return;

  const grid = document.createElement("div");
  grid.className = "settings-grid";
  grid.innerHTML = `
    <div class="settings-tabs">
      <button class="settings-tab active" data-tab="account">Account</button>
      <button class="settings-tab" data-tab="shortcuts">Shortcuts</button>
      <button class="settings-tab" data-tab="app">App</button>
    </div>

    <section class="settings-pane active" data-pane="account">
      <div class="settings-card">
        <div class="settings-info-row"><span>Name</span><strong>${escapeHtml(profile.name || "User")}</strong></div>
        <div class="settings-info-row"><span>Email</span><strong>${escapeHtml(profile.email || "-")}</strong></div>
        <div class="settings-info-row"><span>Initials</span><strong>${escapeHtml(profile.initials || "U")}</strong></div>
        <div class="settings-info-row"><span>Gmail Status</span><strong>${auth.connected ? "Connected" : "Disconnected"}</strong></div>
        <div class="settings-info-row"><span>Inbox</span><strong>${counts.inbox_unread} unread / ${counts.inbox_total} total</strong></div>
        <div class="settings-info-row"><span>Last Inbox Sync</span><strong>${escapeHtml(lastInboxSync)}</strong></div>
      </div>
      <div class="settings-actions">
        <button class="verdant-btn settings-danger" id="settings-logout">Logout</button>
      </div>
    </section>

    <section class="settings-pane" data-pane="shortcuts">
      <label class="settings-switch"><input type="checkbox" id="hk-enabled" ${hotkeys.enabled ? "checked" : ""}> Enable keyboard shortcuts</label>
      <div class="settings-row"><span>Compose</span><input id="hk-compose" value="${escapeHtml(hotkeys.compose)}" /></div>
      <div class="settings-row"><span>Compose Maximize</span><input id="hk-compose-maximize" value="${escapeHtml(hotkeys.composeMaximize)}" /></div>
      <div class="settings-row"><span>Refresh</span><input id="hk-refresh" value="${escapeHtml(hotkeys.refresh)}" /></div>
      <div class="settings-row"><span>Settings</span><input id="hk-settings" value="${escapeHtml(hotkeys.settings)}" /></div>
      <div class="settings-row"><span>Search</span><input id="hk-search" value="${escapeHtml(hotkeys.search)}" /></div>
      <div class="settings-actions">
        <button class="verdant-btn" id="settings-save">Save Shortcuts</button>
      </div>
    </section>

    <section class="settings-pane" data-pane="app">
      <div class="settings-card">
        <div class="settings-info-row"><span>Installed Version</span><strong id="settings-installed-version">Loading...</strong></div>
        <div class="settings-info-row"><span>Update Status</span><strong id="settings-update-status">Not checked yet</strong></div>
        <div class="settings-row"><span>Update Channel</span>
          <select id="update-channel">
            <option value="stable" ${updatePrefs.channel === "stable" ? "selected" : ""}>Stable</option>
            <option value="nightly" ${updatePrefs.channel === "nightly" ? "selected" : ""}>Nightly (beta)</option>
          </select>
        </div>
        <label class="settings-switch"><input type="checkbox" id="update-auto-check" ${updatePrefs.autoCheck ? "checked" : ""}> Automatically check for updates at startup</label>
        <label class="settings-switch"><input type="checkbox" id="update-auto-download" ${updatePrefs.autoDownload ? "checked" : ""}> Automatically download update when available</label>
        <div class="settings-help">
          Verdant keeps a local mail cache database on your device to make loading and searching faster.
          Clearing the local DB only removes cached messages on this device. Your Gmail account and server-side messages are not deleted.
        </div>
      </div>
      <div class="settings-actions">
        <button class="verdant-btn" id="settings-check-update">Check for Updates</button>
        <button class="verdant-btn" id="settings-download-update" disabled>Download Latest Update</button>
        <button class="verdant-btn" id="settings-sync">Sync Emails Now</button>
        <button class="verdant-btn" id="settings-clear">Clear Local DB</button>
      </div>
    </section>
  `;
  panel.appendChild(grid);

  getVersion().then((version) => {
    const el = panel.querySelector("#settings-installed-version");
    if (el) el.textContent = `v${version}`;
  }).catch(() => {
    const el = panel.querySelector("#settings-installed-version");
    if (el) el.textContent = "Unknown";
  });

  const tabs = Array.from(panel.querySelectorAll(".settings-tab"));
  const panes = Array.from(panel.querySelectorAll(".settings-pane"));
  tabs.forEach((tab) => {
    tab.addEventListener("click", () => {
      const target = tab.getAttribute("data-tab");
      tabs.forEach((t) => t.classList.toggle("active", t === tab));
      panes.forEach((pane) => pane.classList.toggle("active", pane.getAttribute("data-pane") === target));
    });
  });

  panel.querySelector("#settings-save")?.addEventListener("click", () => {
    hotkeys = {
      enabled: !!panel.querySelector("#hk-enabled")?.checked,
      compose: normalizeCombo(panel.querySelector("#hk-compose")?.value || defaultHotkeys.compose),
      composeMaximize: normalizeCombo(panel.querySelector("#hk-compose-maximize")?.value || defaultHotkeys.composeMaximize),
      refresh: normalizeCombo(panel.querySelector("#hk-refresh")?.value || defaultHotkeys.refresh),
      settings: normalizeCombo(panel.querySelector("#hk-settings")?.value || defaultHotkeys.settings),
      search: normalizeCombo(panel.querySelector("#hk-search")?.value || defaultHotkeys.search),
      close: "escape",
    };
    saveHotkeys(hotkeys);
    showToast("Shortcuts saved");
  });

  panel.querySelector("#update-auto-check")?.addEventListener("change", (event) => {
    saveUpdatePrefs({ ...updatePrefs, autoCheck: !!event.target?.checked });
  });

  panel.querySelector("#update-channel")?.addEventListener("change", (event) => {
    const value = event.target?.value === "nightly" ? "nightly" : "stable";
    saveUpdatePrefs({ ...updatePrefs, channel: value });
    setSettingsUpdateStatus(`Channel set to ${value}`);
    setSettingsUpdateDownloadButtonEnabled(false);
  });

  panel.querySelector("#update-auto-download")?.addEventListener("change", (event) => {
    saveUpdatePrefs({ ...updatePrefs, autoDownload: !!event.target?.checked });
  });

  panel.querySelector("#settings-check-update")?.addEventListener("click", async () => {
    setSettingsUpdateStatus("Checking for updates...");
    await checkForAppUpdates({ silent: false, autoDownload: false, updateSettingsUi: true, channel: updatePrefs.channel });
  });

  panel.querySelector("#settings-download-update")?.addEventListener("click", async () => {
    setSettingsUpdateStatus("Checking release assets...");
    const info = await checkForAppUpdates({ silent: true, autoDownload: false, updateSettingsUi: true, channel: updatePrefs.channel });
    if (!info?.updateAvailable) { showToast("No update available"); return; }
    setSettingsUpdateStatus("Downloading update package...");
    try {
      const downloaded = await downloadLatestUpdate(updatePrefs.channel);
      setSettingsUpdateStatus(`Downloaded ${downloaded.fileName}`);
      showToast(`Update downloaded: ${downloaded.fileName}`);
    } catch (error) {
      setSettingsUpdateStatus("Update download failed", true);
      showToast(`Update download failed: ${String(error)}`, "error");
    }
  });

  panel.querySelector("#settings-sync")?.addEventListener("click", async () => {
    showToast("Fetching mails...");
    await onSync();
    showToast("Sync complete");
  });

  panel.querySelector("#settings-clear")?.addEventListener("click", async () => {
    await clearLocalData();
    closeOverlay();
    showToast("Local database cleared");
    await onSync();
  });

  panel.querySelector("#settings-logout")?.addEventListener("click", async () => {
    await logout();
    closeOverlay();
    onLogout();
  });
}
