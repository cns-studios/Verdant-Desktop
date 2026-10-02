<script>
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import logoUrl from "../../assets/logo.png";
  import { t } from "../../lib/i18n/index.svelte.js";
  import WindowControls from "./WindowControls.svelte";

  let { subtitle } = $props();

  const outsideControls = (event) => !event.target.closest(".app-header-controls");

  function startDrag(event) {
    if (event.button !== 0 || event.detail > 1 || !outsideControls(event)) return;
    getCurrentWindow().startDragging().catch(() => {});
  }

  function toggleMaximize(event) {
    if (outsideControls(event)) getCurrentWindow().toggleMaximize().catch(() => {});
  }
</script>

<header class="app-header" onpointerdown={startDrag} ondblclick={toggleMaximize}>
  <div class="app-header-left">
    <img class="app-logo-mark" src={logoUrl} alt="" draggable="false" />
    <span class="app-title">{t("app.title")}</span>
    <span class="app-subtitle">- {subtitle}</span>
  </div>
  <div class="app-header-controls">
    <WindowControls />
  </div>
</header>
