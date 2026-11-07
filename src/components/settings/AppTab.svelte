<script>
  import { onMount } from "svelte";
  import { getLang, LANGUAGES, setLang, t } from "../../lib/i18n/index.svelte.js";
  import { mail } from "../../state/mail.svelte.js";
  import { session } from "../../state/session.svelte.js";
  import { ui } from "../../state/ui.svelte.js";
  import { updates } from "../../state/updates.svelte.js";
  import SettingsItem from "./SettingsItem.svelte";
  import SettingsSection from "./SettingsSection.svelte";

  const inboxStatus = $derived(
    t("settings.app.inbox_status", { unread: mail.counts.inbox_unread, total: mail.counts.inbox_total }),
  );
  const lastSync = $derived.by(() => {
    const at = mail.lastSyncedAt("INBOX");
    return at ? new Date(at).toLocaleString() : t("settings.app.not_synced");
  });

  async function remove(account) {
    const wasActive = account.is_active;
    if ((await session.removeAccount(account)) && wasActive) ui.settingsOpen = false;
  }

  onMount(() => {
    updates.resetStatus();
    updates.loadVersion();
    session.refreshAccounts();
    mail.refreshCounts();
  });
</script>

<SettingsSection title={t("settings.section.general")}>
  <SettingsItem label={t("settings.app.language")}>
    <select class="set-select" value={getLang()} onchange={(event) => setLang(event.currentTarget.value)}>
      {#each LANGUAGES as language (language.code)}
        <option value={language.code}>{language.label}</option>
      {/each}
    </select>
  </SettingsItem>
</SettingsSection>

<SettingsSection title={t("settings.section.accounts")}>
  {#each session.accounts as account (account.id)}
    <SettingsItem
      label={account.display_name || account.email}
      active={account.is_active}
      help={account.is_active ? `${account.provider} · ${inboxStatus}` : account.provider}
      title={account.is_active ? `${t("settings.app.last_sync")}: ${lastSync}` : undefined}
    >
      <button class="verdant-btn settings-danger" onclick={() => remove(account)}>
        {t(account.is_active ? "settings.app.logout" : "settings.app.remove_account")}
      </button>
    </SettingsItem>
  {/each}
</SettingsSection>

<SettingsSection title={t("settings.section.updates")}>
  <SettingsItem
    label={t("settings.app.installed_version")}
    help={updates.status.text || t("settings.app.update_not_checked")}
    error={!!updates.status.error}
  >
    <span class="set-value muted">v{updates.version || t("app.version_unknown")}</span>
    <button
      class={["verdant-btn", updates.ready && "primary", updates.busy && "busy"]}
      disabled={updates.busy}
      onclick={() => (updates.ready ? updates.installFromSettings() : updates.checkFromSettings())}
    >
      {#if updates.busy}<span class="btn-spinner" aria-hidden="true"></span>{/if}
      <span class="btn-label">{t(updates.ready ? "settings.app.download_update" : "settings.app.check_update")}</span>
    </button>
  </SettingsItem>
</SettingsSection>
