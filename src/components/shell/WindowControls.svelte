<script>
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { t } from "../../lib/i18n/index.svelte.js";
  import Icon from "../ui/Icon.svelte";

  let { buttonClass = "app-win-btn" } = $props();

  const run = (action) => () => action(getCurrentWindow()).catch(() => {});
</script>

<button class={buttonClass} aria-label={t("app.minimize_window")} title={t("app.minimize")} onclick={run((w) => w.minimize())}>
  <Icon name="minus" />
</button>
<button class={buttonClass} aria-label={t("app.maximize_window")} title={t("app.maximize")} onclick={run((w) => w.toggleMaximize())}>
  <Icon name="square" />
</button>
<button class={[buttonClass, "close"]} aria-label={t("app.close_window")} title={t("app.close")} onclick={run((w) => w.close())}>
  <Icon name="x" />
</button>
