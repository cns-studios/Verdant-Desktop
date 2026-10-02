<script>
  import { EDITABLE_HOTKEYS, eventCombo, formatCombo } from "../../lib/hotkeys.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { prefs, saveHotkeys } from "../../state/prefs.svelte.js";
  import { showToast } from "../../state/ui.svelte.js";

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

<label class="settings-switch" style:margin-bottom="10px">
  <input
    type="checkbox"
    checked={prefs.hotkeys.enabled}
    onchange={(event) => saveHotkeys({ enabled: event.currentTarget.checked })}
  />
  {t("settings.shortcuts.enabled")}
</label>
<div class="settings-shortcut-list">
  {#each EDITABLE_HOTKEYS as shortcut (shortcut.key)}
    {@const combo = prefs.hotkeys[shortcut.key]}
    <div class="settings-shortcut-row">
      <span class="settings-shortcut-label">{t(shortcut.label)}</span>
      <div class="settings-shortcut-controls">
        <span class={["settings-shortcut-key", listening === shortcut.key && "listening"]}>
          {listening === shortcut.key ? t("settings.shortcuts.listening") : formatCombo(combo)}
        </span>
        <button class="verdant-btn settings-shortcut-edit" onclick={() => (listening = shortcut.key)}>
          {t("settings.shortcuts.edit")}
        </button>
        <button class="verdant-btn settings-shortcut-unset" disabled={!combo} onclick={() => set(shortcut.key, "")}>
          {t("settings.shortcuts.unset")}
        </button>
      </div>
    </div>
  {/each}
</div>
