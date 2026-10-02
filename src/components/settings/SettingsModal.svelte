<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import { ui } from "../../state/ui.svelte.js";
  import Overlay from "../ui/Overlay.svelte";
  import AdvancedTab from "./AdvancedTab.svelte";
  import AppearanceTab from "./AppearanceTab.svelte";
  import AppTab from "./AppTab.svelte";
  import BehaviorTab from "./BehaviorTab.svelte";
  import ShortcutsTab from "./ShortcutsTab.svelte";
  import SmartInboxTab from "./SmartInboxTab.svelte";

  const TABS = [
    { id: "app", label: "settings.tab.app", component: AppTab },
    { id: "behavior", label: "settings.tab.behavior", component: BehaviorTab },
    { id: "appearence", label: "settings.tab.appearence", component: AppearanceTab },
    { id: "smart", label: "settings.tab.smart", component: SmartInboxTab },
    { id: "shortcuts", label: "settings.tab.shortcuts", component: ShortcutsTab },
    { id: "advanced", label: "settings.tab.advanced", component: AdvancedTab },
  ];

  let current = $state("app");
  let tabButtons = $state([]);

  const Pane = $derived(TABS.find((tab) => tab.id === current).component);
  const close = () => (ui.settingsOpen = false);

  function cycleTabs(event) {
    if (event.key !== "Tab") return;
    event.preventDefault();
    const index = TABS.findIndex((tab) => tab.id === current);
    const next = (index + (event.shiftKey ? -1 : 1) + TABS.length) % TABS.length;
    current = TABS[next].id;
    tabButtons[next]?.focus();
  }
</script>

<svelte:document onkeydown={cycleTabs} />

<Overlay onclose={close} label={t("settings.title")}>
  <div class="verdant-head">
    <h2>{t("settings.title")}</h2>
    <button class="verdant-close" aria-label={t("reading.close")} onclick={close}>×</button>
  </div>
  <div class="settings-grid">
    <div class="settings-tabs">
      {#each TABS as tab, index (tab.id)}
        <button
          class={["settings-tab", current === tab.id && "active"]}
          bind:this={tabButtons[index]}
          onclick={() => (current = tab.id)}
        >
          {t(tab.label)}
        </button>
      {/each}
    </div>
    <section class="settings-pane active">
      <Pane />
    </section>
  </div>
</Overlay>
