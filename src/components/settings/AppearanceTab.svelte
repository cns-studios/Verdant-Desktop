<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import { prefs, saveAppPrefs, TEXT_SIZES, THEMES } from "../../state/prefs.svelte.js";
  import SettingsItem from "./SettingsItem.svelte";
  import SettingsSection from "./SettingsSection.svelte";

  const THEME_PULSE_MS = 360;

  const sizeIndex = $derived(TEXT_SIZES.indexOf(prefs.app.textSize));

  function setTheme(theme) {
    saveAppPrefs({ theme, useDarkMode: theme !== "light" });
    document.documentElement.classList.add("theme-switching");
    setTimeout(() => document.documentElement.classList.remove("theme-switching"), THEME_PULSE_MS);
  }
</script>

<SettingsSection title={t("settings.section.theme")}>
  <div class="theme-cards" role="radiogroup" aria-label={t("settings.appearance.title")}>
    {#each THEMES as theme (theme)}
      <label class={["theme-card", prefs.app.theme === theme && "active"]}>
        <input
          type="radio"
          name="colorscheme"
          value={theme}
          checked={prefs.app.theme === theme}
          onchange={() => setTheme(theme)}
        />
        <span class="theme-preview theme-preview-{theme}" aria-hidden="true">
          <i class="tp-side"></i>
          <i class="tp-list"><b></b><b></b><b></b></i>
          <i class="tp-read"><b></b><b></b></i>
        </span>
        <span class="theme-card-label">{t(`settings.appearence.${theme}`)}</span>
      </label>
    {/each}
  </div>
</SettingsSection>

<SettingsSection>
  <SettingsItem label={t("settings.appearance.text_size")} help={t("settings.appearance.text_size_description")}>
    <strong class="set-value">{t(`settings.appearance.text_size.${prefs.app.textSize}`)}</strong>
    {#snippet below()}
      <input
        class="settings-range"
        type="range"
        min="0"
        max={TEXT_SIZES.length - 1}
        step="1"
        value={sizeIndex}
        style:--range-position="{sizeIndex * 50}%"
        aria-label={t("settings.appearance.text_size")}
        oninput={(event) => saveAppPrefs({ textSize: TEXT_SIZES[Number(event.currentTarget.value)] })}
      />
      <div class="settings-range-labels" aria-hidden="true">
        {#each TEXT_SIZES as size (size)}
          <span>{t(`settings.appearance.text_size.${size}`)}</span>
        {/each}
      </div>
    {/snippet}
  </SettingsItem>
</SettingsSection>
