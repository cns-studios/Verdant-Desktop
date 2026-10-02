import { listen } from "@tauri-apps/api/event";
import * as api from "../lib/api.js";
import { loadContacts } from "../lib/contacts.js";
import { t } from "../lib/i18n/index.svelte.js";
import { mail } from "./mail.svelte.js";
import { syncPrefsWithBackend } from "./prefs.svelte.js";
import { smart } from "./smartInbox.svelte.js";
import { showToast } from "./ui.svelte.js";
import { updates } from "./updates.svelte.js";

const MAX_PROFILE_RETRIES = 8;

class Session {
  status = $state("booting");
  error = $state("");
  profile = $state({ name: "", email: "", initials: "?" });
  accounts = $state([]);

  #profileTimer = null;
  #listening = false;

  get activeAccount() {
    return this.accounts.find((account) => account.is_active) ?? null;
  }

  async boot() {
    this.status = "booting";
    try {
      const flags = await api.getStartupFlags().catch(() => ({ is_autostart: false }));
      if (flags.is_autostart) api.hideMainWindow().catch((error) => console.error("Autostart hide failed", error));

      const [auth] = await Promise.all([api.authStatus(), syncPrefsWithBackend(), loadContacts()]);
      if (!auth.has_client_id) this.status = "config-missing";
      else if (!auth.connected) this.status = "onboarding";
      else await this.start();
    } catch (error) {
      this.error = String(error);
      this.status = "error";
    }
  }

  async start() {
    this.status = "ready";
    if (!this.#listening) {
      this.#listening = true;
      await listen("emails-synced", () => mail.reload());
      updates.startBackgroundChecks();
    }
    await Promise.all([
      this.refreshProfile(),
      this.refreshAccounts(),
      smart.refresh(),
      mail.open("INBOX", { sync: false }),
    ]);
  }

  async refreshAccounts() {
    try {
      this.accounts = await api.listAccounts();
    } catch (error) {
      console.error("Failed to list accounts", error);
    }
  }

  async refreshProfile(attempt = 0) {
    clearTimeout(this.#profileTimer);
    try {
      const profile = await api.getUserProfile();
      this.profile = profile;
      if (!profile.degraded) return;
      if (attempt === 0) showToast(t("toast.rate_limited"));
    } catch (error) {
      console.error("Failed to load profile", error);
    }
    if (attempt >= MAX_PROFILE_RETRIES) return;
    const delay = Math.min(1000 * 2 ** attempt, 60000);
    this.#profileTimer = setTimeout(() => this.refreshProfile(attempt + 1), delay);
  }

  async switchAccount(accountId) {
    try {
      await api.switchAccount(accountId);
    } catch (error) {
      showToast(String(error), "error");
      return;
    }
    mail.reset();
    await Promise.all([this.refreshProfile(), this.refreshAccounts(), smart.refresh()]);
    await mail.open("INBOX");
  }

  async switchToNextAccount() {
    await this.refreshAccounts();
    if (this.accounts.length === 0) return showToast(t("accounts.switch_none"));
    if (this.accounts.length === 1) return showToast(t("accounts.switch_single"));
    const current = this.accounts.findIndex((account) => account.is_active);
    const next = this.accounts[(current + 1) % this.accounts.length];
    showToast(t("accounts.switched", { account: next.display_name || next.email }));
    await this.switchAccount(next.id);
  }

  async accountAdded(account) {
    if (this.status !== "ready") return this.start();
    if (account?.id) return this.switchAccount(account.id);
    await this.refreshAccounts();
  }

  async removeAccount(account) {
    if (!confirm(t("accounts.confirm_remove", { email: account.email }))) return false;
    try {
      await api.removeAccount(account.id);
    } catch (error) {
      showToast(String(error), "error");
      return false;
    }
    showToast(t("accounts.removed"));
    await this.refreshAccounts();
    if (!account.is_active) return true;

    const next = this.activeAccount ?? this.accounts[0];
    if (next) {
      await this.switchAccount(next.id);
    } else {
      mail.reset();
      this.status = "onboarding";
    }
    return true;
  }
}

export const session = new Session();
