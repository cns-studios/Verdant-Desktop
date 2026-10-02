<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import { prefs, saveAppPrefs, TEXT_SIZES } from "../../state/prefs.svelte.js";

  const THEMES = [
    { id: "light", dark: false },
    { id: "dark", dark: true },
  ];
  const THEME_PULSE_MS = 360;

  const sizeIndex = $derived(TEXT_SIZES.indexOf(prefs.app.textSize));

  function setTheme(dark) {
    saveAppPrefs({ useDarkMode: dark });
    document.documentElement.classList.add("theme-switching");
    setTimeout(() => document.documentElement.classList.remove("theme-switching"), THEME_PULSE_MS);
  }
</script>

<div class="settings-section-label">{t("settings.appearance.title")}</div>
<div class="settings-card">
  <div class="theme-tabs" role="tablist" aria-label={t("settings.appearance.title")}>
    {#each THEMES as theme (theme.id)}
      <label class={["theme-tab", prefs.app.useDarkMode === theme.dark && "active"]}>
        <input
          type="radio"
          name="colorscheme"
          value={theme.id}
          checked={prefs.app.useDarkMode === theme.dark}
          onchange={() => setTheme(theme.dark)}
        />
        <span class="theme-tab-icon theme-tab-icon-{theme.id}" aria-hidden="true"></span>
        <span>{t(`settings.appearence.${theme.id}`)}</span>
      </label>
    {/each}
  </div>
</div>

<div class="settings-section-label">{t("settings.appearance.text_size")}</div>
<div class="settings-card settings-text-size-card">
  <div class="settings-text-size-heading">
    <span>{t("settings.appearance.text_size_description")}</span>
    <strong>{t(`settings.appearance.text_size.${prefs.app.textSize}`)}</strong>
  </div>
  <div class="settings-range-wrap">
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
  </div>
</div>
