<script>
  import { onMount } from "svelte";
  import { getLang, LANGUAGES, setLang, t } from "../../lib/i18n/index.svelte.js";
  import { mail } from "../../state/mail.svelte.js";
  import { session } from "../../state/session.svelte.js";
  import { ui } from "../../state/ui.svelte.js";
  import { updates } from "../../state/updates.svelte.js";

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

<div class="settings-section-label">{t("settings.app.language")}</div>
<div class="settings-card">
  <div class="settings-row">
    <span>{t("settings.app.language")}</span>
    <select value={getLang()} onchange={(event) => setLang(event.currentTarget.value)}>
      {#each LANGUAGES as language (language.code)}
        <option value={language.code}>{language.label}</option>
      {/each}
    </select>
  </div>
</div>

<div class="settings-section-label">{t("settings.app.connected_inboxes")}</div>
<div class="settings-inbox-list">
  {#each session.accounts as account (account.id)}
    <div
      class="settings-inbox-item"
      title={account.is_active ? `${inboxStatus} · ${t("settings.app.last_sync")}: ${lastSync}` : null}
    >
      <div class="settings-inbox-main">
        <strong>{account.display_name || account.email}</strong>
        <span class="settings-inbox-provider">
          {account.provider}{#if account.is_active}&nbsp;· {inboxStatus}{/if}
        </span>
      </div>
      <div class="settings-inbox-actions">
        <button class="verdant-btn settings-danger" onclick={() => remove(account)}>
          {t(account.is_active ? "settings.app.logout" : "settings.app.remove_account")}
        </button>
      </div>
    </div>
  {/each}
</div>

<div class="settings-section-label">{t("settings.app.update")}</div>
<div class="settings-card">
  <div class="settings-info-row">
    <span>{t("settings.app.installed_version")}</span>
    <strong>v{updates.version || t("app.version_unknown")}</strong>
  </div>
  <div class="settings-info-row">
    <span>{t("settings.app.update_status")}</span>
    <strong style:color={updates.status.error ? "#8a3b3b" : null}>
      {updates.status.text || t("settings.app.update_not_checked")}
    </strong>
  </div>
</div>
<div class="settings-actions">
  <button
    class="verdant-btn"
    disabled={updates.busy}
    onclick={() => (updates.ready ? updates.installFromSettings() : updates.checkFromSettings())}
  >
    {#if updates.busy}
      {updates.busyLabel}
    {:else}
      {t(updates.ready ? "settings.app.download_update" : "settings.app.check_update")}
    {/if}
  </button>
</div>
