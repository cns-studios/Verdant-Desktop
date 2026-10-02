import { getVersion } from "@tauri-apps/api/app";
import { exit } from "@tauri-apps/plugin-process";
import { Store } from "@tauri-apps/plugin-store";
import * as api from "../lib/api.js";
import { t } from "../lib/i18n/index.svelte.js";
import { readText, writeText } from "../lib/storage.js";
import { prefs } from "./prefs.svelte.js";
import { showToast, ui } from "./ui.svelte.js";

const LAST_NIGHTLY_URL_KEY = "verdant.lastNightlyUrl";
const PERIODIC_CHECK_MS = 30 * 60 * 1000;
const STORE_FILE = "verdant.json";

const channelLabel = (channel) =>
  t(channel === "nightly" ? "settings.advanced.channel.nightly" : "settings.advanced.channel.stable");

class Updates {
  version = $state("");
  status = $state({ text: "", error: false });
  ready = $state(false);
  busy = $state(false);
  busyLabel = $state("");
  offer = $state(null);

  async loadVersion() {
    if (!this.version) this.version = await getVersion().catch(() => "");
    return this.version;
  }

  resetStatus() {
    this.status = { text: "", error: false };
    this.ready = false;
  }

  async #check() {
    const channel = prefs.update.channel;
    const info = await api.checkForUpdates(channel);
    if (channel === "nightly" && info.updateAvailable) {
      if (info.downloadUrl === readText(LAST_NIGHTLY_URL_KEY)) info.updateAvailable = false;
      else writeText(LAST_NIGHTLY_URL_KEY, info.downloadUrl);
    }
    return info;
  }

  async checkFromSettings() {
    const channel = channelLabel(prefs.update.channel);
    this.busy = true;
    this.busyLabel = t("settings.app.checking");
    this.status = { text: t("settings.app.checking"), error: false };
    try {
      const info = await this.#check();
      this.ready = info.updateAvailable;
      if (info.updateAvailable) {
        this.status = { text: t("settings.app.update_available_status", { channel, version: info.latestVersion }), error: false };
        showToast(t("toast.update_available", { channel, version: info.latestVersion }));
      } else {
        this.status = { text: t("settings.app.up_to_date", { channel, version: info.currentVersion }), error: false };
        showToast(t("toast.no_update"));
      }
    } catch (error) {
      this.status = { text: t("settings.app.check_failed"), error: true };
      showToast(`${t("settings.app.check_failed")}: ${error}`, "error");
    } finally {
      this.busy = false;
    }
  }

  async installFromSettings() {
    this.busy = true;
    this.busyLabel = t("settings.app.downloading");
    this.status = { text: t("settings.app.downloading"), error: false };
    try {
      await this.#downloadAndInstall((phase, detail) => {
        this.busyLabel = t(`update.${phase}`);
        if (phase === "installing") {
          this.status = { text: t("toast.update_downloaded", { file: detail }), error: false };
          showToast(this.status.text);
        }
      });
    } catch (error) {
      this.status = { text: t("settings.app.download_failed"), error: true };
      showToast(`${t("settings.app.download_failed")}: ${error}`, "error");
      this.busy = false;
    }
  }

  async installOffer() {
    this.offer.phase = "downloading";
    try {
      await this.#downloadAndInstall((phase) => (this.offer.phase = phase));
    } catch (error) {
      this.offer.phase = "failed";
      this.offer.error = String(error);
    }
  }

  dismissOffer() {
    this.offer = null;
  }

  startBackgroundChecks() {
    this.#offerUpdate();
    this.#showWhatsNewOnce();
    setInterval(() => this.#automaticCheck(), PERIODIC_CHECK_MS);
  }

  async openWhatsNew(force = false) {
    const version = await this.loadVersion();
    if (!version || ui.whatsNew) return;
    const store = await Store.load(STORE_FILE);
    const dismissed = (await store.get("whatsNewDismissed")) || [];
    if (!force && dismissed.includes(version)) return;
    const content = await api.getChangelog(version).then((entry) => entry.content).catch(() => null);
    ui.whatsNew = { version, content };
  }

  async closeWhatsNew(dismiss) {
    const version = ui.whatsNew?.version;
    ui.whatsNew = null;
    if (!dismiss || !version) return;
    try {
      const store = await Store.load(STORE_FILE);
      const dismissed = (await store.get("whatsNewDismissed")) || [];
      if (dismissed.includes(version)) return;
      await store.set("whatsNewDismissed", [...dismissed, version]);
      await store.save();
    } catch (error) {
      console.error("Failed to remember dismissed release notes", error);
    }
  }

  async #downloadAndInstall(onPhase) {
    onPhase("downloading");
    const downloaded = await api.downloadLatestUpdate(prefs.update.channel);
    onPhase("installing", downloaded.fileName);
    await api.installAndRelaunch(downloaded.filePath);
    onPhase("restarting");
    await new Promise((resolve) => setTimeout(resolve, 800));
    await exit(0);
  }

  async #offerUpdate() {
    try {
      const info = await this.#check();
      if (info.updateAvailable) this.offer = { info, phase: "offer", error: "" };
    } catch {}
  }

  async #automaticCheck() {
    if (!prefs.update.autoCheck) return;
    try {
      const info = await this.#check();
      if (!info.updateAvailable || !prefs.update.autoDownload) return;
      const downloaded = await api.downloadLatestUpdate(prefs.update.channel);
      showToast(t("toast.update_downloaded", { file: downloaded.fileName }));
    } catch (error) {
      console.error("Periodic update check failed", error);
    }
  }

  async #showWhatsNewOnce() {
    try {
      const version = await this.loadVersion();
      const store = await Store.load(STORE_FILE);
      if (!version || (await store.get("lastSeenVersion")) === version) return;
      await this.openWhatsNew();
      await store.set("lastSeenVersion", version);
      await store.save();
    } catch (error) {
      console.error("Failed to check for release notes", error);
    }
  }
}

export const updates = new Updates();
