<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import { ui } from "../../state/ui.svelte.js";
  import Icon from "../ui/Icon.svelte";
  import Overlay from "../ui/Overlay.svelte";
  import AdvancedTab from "./AdvancedTab.svelte";
  import AppearanceTab from "./AppearanceTab.svelte";
  import AppTab from "./AppTab.svelte";
  import BehaviorTab from "./BehaviorTab.svelte";
  import ShortcutsTab from "./ShortcutsTab.svelte";
  import SmartInboxTab from "./SmartInboxTab.svelte";

  const TABS = [
    { id: "app", icon: "settings-app", label: "settings.tab.app", component: AppTab },
    { id: "behavior", icon: "settings-behavior", label: "settings.tab.behavior", component: BehaviorTab },
    { id: "appearence", icon: "settings-appearance", label: "settings.tab.appearence", component: AppearanceTab },
    { id: "smart", icon: "sparkles", label: "settings.tab.smart", component: SmartInboxTab },
    { id: "shortcuts", icon: "settings-shortcuts", label: "settings.tab.shortcuts", component: ShortcutsTab },
    { id: "advanced", icon: "settings-advanced", label: "settings.tab.advanced", component: AdvancedTab },
  ];

  let current = $state("app");
  let tabButtons = $state([]);

  const active = $derived(TABS.find((tab) => tab.id === current));
  const Pane = $derived(active.component);
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

<Overlay onclose={close} panelClass="settings-panel" label={t("settings.title")}>
  <div class="settings-shell">
    <header class="settings-top">
      <div class="settings-head">
        <h2 class="settings-title">{t("settings.title")}</h2>
        <button class="verdant-close" aria-label={t("reading.close")} onclick={close}><Icon name="x" size={16} /></button>
      </div>
      <nav class="settings-tabs" role="tablist">
        {#each TABS as tab, index (tab.id)}
          <button
            role="tab"
            aria-selected={current === tab.id}
            class={["settings-tab", current === tab.id && "active"]}
            bind:this={tabButtons[index]}
            onclick={() => (current = tab.id)}
          >
            <Icon name={tab.icon} size={15} />
            <span class="settings-tab-label">{t(tab.label)}</span>
          </button>
        {/each}
      </nav>
    </header>
    <section class="settings-pane">
      <div class="settings-column">
        <Pane />
      </div>
    </section>
  </div>
</Overlay>
