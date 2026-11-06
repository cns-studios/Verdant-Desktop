<script>
  import { autostartDisable, autostartEnable } from "../../lib/api.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { prefs, saveAppPrefs } from "../../state/prefs.svelte.js";
  import { showToast } from "../../state/ui.svelte.js";
  import SettingsItem from "./SettingsItem.svelte";
  import SettingsSection from "./SettingsSection.svelte";
  import Toggle from "./Toggle.svelte";

  const IMPORTANCE = [
    { value: "all", label: "settings.behavior.all_mail" },
    { value: "important", label: "settings.behavior.only_important" },
  ];

  const importance = $derived(prefs.app.notificationImportance === "important" ? "important" : "all");

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

<SettingsSection title={t("settings.section.notifications")}>
  <SettingsItem label={t("settings.behavior.notifications")} help={t("settings.behavior.notifications_help")}>
    <Toggle
      label={t("settings.behavior.notifications")}
      checked={prefs.app.showNotifications}
      onchange={(on) => saveAppPrefs({ showNotifications: on })}
    />
    {#snippet below()}
      <div class={["set-segmented", !prefs.app.showNotifications && "disabled"]} role="radiogroup">
        {#each IMPORTANCE as option (option.value)}
          <label class={["set-segment", importance === option.value && "active"]}>
            <input
              type="radio"
              name="notification-importance"
              value={option.value}
              checked={importance === option.value}
              disabled={!prefs.app.showNotifications}
              onchange={() => saveAppPrefs({ notificationImportance: option.value })}
            />
            {t(option.label)}
          </label>
        {/each}
      </div>
    {/snippet}
  </SettingsItem>
</SettingsSection>

<SettingsSection title={t("settings.section.privacy")}>
  <SettingsItem label={t("settings.behavior.load_images")} help={t("settings.behavior.load_images_help")}>
    <Toggle
      label={t("settings.behavior.load_images")}
      checked={prefs.app.loadImages}
      onchange={(on) => saveAppPrefs({ loadImages: on })}
    />
  </SettingsItem>
  <SettingsItem label={t("settings.behavior.discord_rpc")} help={t("settings.behavior.discord_rpc_help")}>
    <Toggle
      label={t("settings.behavior.discord_rpc")}
      checked={prefs.app.discordRpc}
      onchange={(on) => saveAppPrefs({ discordRpc: on })}
    />
  </SettingsItem>
</SettingsSection>

<SettingsSection title={t("settings.section.startup")}>
  <SettingsItem label={t("settings.behavior.start_on_login")}>
    <Toggle label={t("settings.behavior.start_on_login")} checked={prefs.app.autostart} onchange={setAutostart} />
  </SettingsItem>
  <SettingsItem label={t("settings.behavior.whats_new")} help={t("settings.behavior.whats_new_help")}>
    <Toggle
      label={t("settings.behavior.whats_new")}
      checked={prefs.app.showWhatsNew !== false}
      onchange={(on) => saveAppPrefs({ showWhatsNew: on })}
    />
  </SettingsItem>
  <SettingsItem label={t("settings.behavior.run_in_background")}>
    <Toggle
      label={t("settings.behavior.run_in_background")}
      checked={prefs.app.runInBackground}
      onchange={(on) => saveAppPrefs({ runInBackground: on })}
    />
  </SettingsItem>
</SettingsSection>
