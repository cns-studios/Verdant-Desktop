<script>
  import { autostartDisable, autostartEnable } from "../../lib/api.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { prefs, saveAppPrefs } from "../../state/prefs.svelte.js";
  import { showToast } from "../../state/ui.svelte.js";

  const IMPORTANCE = [
    { value: "all", label: "settings.behavior.all_mail" },
    { value: "important", label: "settings.behavior.only_important" },
  ];

  async function setAutostart(enabled) {
    saveAppPrefs({ autostart: enabled });
    try {
      await (enabled ? autostartEnable() : autostartDisable());
    } catch (error) {
      console.error("Failed to toggle autostart", error);
      showToast(t("settings.app.check_failed"), "error");
    }
  }
</script>

<div class="settings-section-label">{t("settings.behavior.notifications")}</div>
<div class="settings-card">
  <label class="settings-switch">
    <input
      type="checkbox"
      checked={prefs.app.showNotifications}
      onchange={(event) => saveAppPrefs({ showNotifications: event.currentTarget.checked })}
    />
    {t("settings.behavior.notifications")}
  </label>
  <div
    class="settings-radio-group"
    style:margin-top="6px"
    style:opacity={prefs.app.showNotifications ? null : 0.5}
    style:pointer-events={prefs.app.showNotifications ? null : "none"}
  >
    {#each IMPORTANCE as option (option.value)}
      <label class="settings-radio">
        <input
          type="radio"
          name="notification-importance"
          value={option.value}
          checked={(prefs.app.notificationImportance === "important") === (option.value === "important")}
          onchange={() => saveAppPrefs({ notificationImportance: option.value })}
        />
        {t(option.label)}
      </label>
    {/each}
  </div>
</div>

<div class="settings-section-label">{t("settings.behavior.start_on_login")}</div>
<div class="settings-card">
  <label class="settings-switch">
    <input type="checkbox" checked={prefs.app.autostart} onchange={(event) => setAutostart(event.currentTarget.checked)} />
    {t("settings.behavior.start_on_login")}
  </label>
  <label class="settings-switch">
    <input
      type="checkbox"
      checked={prefs.app.runInBackground}
      onchange={(event) => saveAppPrefs({ runInBackground: event.currentTarget.checked })}
    />
    {t("settings.behavior.run_in_background")}
  </label>
</div>
