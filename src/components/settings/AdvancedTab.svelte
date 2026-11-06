<script>
  import { clearLocalData, syncEmails } from "../../lib/api.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { mail } from "../../state/mail.svelte.js";
  import { prefs, saveUpdatePrefs } from "../../state/prefs.svelte.js";
  import { showToast, ui } from "../../state/ui.svelte.js";
  import { updates } from "../../state/updates.svelte.js";
  import SettingsItem from "./SettingsItem.svelte";
  import SettingsSection from "./SettingsSection.svelte";

  const CHANNELS = ["stable", "nightly"];

  let syncing = $state(false);

  function setChannel(channel) {
    saveUpdatePrefs({ channel });
    updates.resetStatus();
    showToast(t("settings.app.channel_set", { channel: t(`settings.advanced.channel.${channel}`) }));
  }

  async function syncAll() {
    if (syncing) return;
    syncing = true;
    showToast(t("toast.fetching"));
    try {
      await syncEmails();
      showToast(t("toast.sync_complete"));
    } catch (error) {
      showToast(String(error), "error");
    } finally {
      syncing = false;
    }
  }

  async function clearCache() {
    await clearLocalData();
    ui.settingsOpen = false;
    showToast(t("toast.db_cleared"));
    mail.reset();
    await mail.open("INBOX", { animate: false });
  }
</script>

<SettingsSection title={t("settings.section.updates")}>
  <SettingsItem label={t("settings.advanced.update_channel")} help={t("settings.advanced.channel_info")}>
    <select class="set-select" value={prefs.update.channel} onchange={(event) => setChannel(event.currentTarget.value)}>
      {#each CHANNELS as channel (channel)}
        <option value={channel}>{t(`settings.advanced.channel.${channel}`)}</option>
      {/each}
    </select>
  </SettingsItem>
</SettingsSection>

<SettingsSection title={t("settings.section.data")}>
  <SettingsItem help={t("settings.advanced.cache_info")}>
    {#snippet below()}
      <div class="set-actions">
        <button class="verdant-btn" disabled={syncing} onclick={syncAll}>{t("settings.advanced.sync_all")}</button>
        <button class="verdant-btn settings-danger" onclick={clearCache}>{t("settings.advanced.clear_cache")}</button>
      </div>
    {/snippet}
  </SettingsItem>
</SettingsSection>
