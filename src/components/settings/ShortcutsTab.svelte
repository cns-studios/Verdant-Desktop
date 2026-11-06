<script>
  import { EDITABLE_HOTKEYS, eventCombo, formatCombo } from "../../lib/hotkeys.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { prefs, saveHotkeys } from "../../state/prefs.svelte.js";
  import { showToast } from "../../state/ui.svelte.js";
  import Icon from "../ui/Icon.svelte";
  import SettingsItem from "./SettingsItem.svelte";
  import SettingsSection from "./SettingsSection.svelte";
  import Toggle from "./Toggle.svelte";

  const MODIFIER_KEYS = ["Control", "Shift", "Alt", "Meta"];

  let listening = $state(null);

  function capture(event) {
    if (!listening) return;
    event.stopPropagation();
    if (event.key === "Escape") {
      listening = null;
      return;
    }
    if (MODIFIER_KEYS.includes(event.key)) return;
    event.preventDefault();
    set(listening, eventCombo(event));
    listening = null;
  }

  function set(key, combo) {
    saveHotkeys({ [key]: combo });
    showToast(t("toast.shortcuts_saved"));
  }
</script>

<svelte:document onkeydowncapture={capture} />

<SettingsSection>
  <SettingsItem label={t("settings.shortcuts.enabled")}>
    <Toggle
      label={t("settings.shortcuts.enabled")}
      checked={prefs.hotkeys.enabled}
      onchange={(on) => saveHotkeys({ enabled: on })}
    />
  </SettingsItem>
</SettingsSection>

<SettingsSection>
  {#each EDITABLE_HOTKEYS as shortcut (shortcut.key)}
    {@const combo = prefs.hotkeys[shortcut.key]}
    <SettingsItem label={t(shortcut.label)} disabled={!prefs.hotkeys.enabled}>
      <button
        class={["settings-shortcut-key", listening === shortcut.key && "listening", !combo && "empty"]}
        title={t("settings.shortcuts.edit")}
        onclick={() => (listening = shortcut.key)}
      >
        {listening === shortcut.key ? t("settings.shortcuts.listening") : combo ? formatCombo(combo) : "—"}
      </button>
      <button
        class="set-icon-btn"
        title={t("settings.shortcuts.unset")}
        aria-label={t("settings.shortcuts.unset")}
        disabled={!combo}
        onclick={() => set(shortcut.key, "")}
      >
        <Icon name="x" size={14} />
      </button>
    </SettingsItem>
  {/each}
</SettingsSection>
