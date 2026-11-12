<script>
  import { tick } from "svelte";
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
  import SubscriptionsTab from "./SubscriptionsTab.svelte";

  const TABS = [
    { id: "app", icon: "settings-app", label: "settings.tab.app", component: AppTab },
    { id: "behavior", icon: "settings-behavior", label: "settings.tab.behavior", component: BehaviorTab },
    { id: "appearence", icon: "settings-appearance", label: "settings.tab.appearence", component: AppearanceTab },
    { id: "smart", icon: "sparkles", label: "settings.tab.smart", component: SmartInboxTab },
    { id: "subscriptions", icon: "subscriptions", label: "settings.tab.subscriptions", component: SubscriptionsTab },
    { id: "shortcuts", icon: "settings-shortcuts", label: "settings.tab.shortcuts", component: ShortcutsTab },
    { id: "advanced", icon: "settings-advanced", label: "settings.tab.advanced", component: AdvancedTab },
  ];

  let current = $state("app");
  let tabButtons = $state([]);

  let query = $state("");
  let column = $state();
  let searchInput = $state();
  const searching = $derived(query.trim().length > 0);

  $effect(() => {
    const q = query.trim().toLowerCase();
    if (!column) return;
    tick().then(() => {
      for (const group of column.querySelectorAll(".set-group")) group.hidden = false;
      for (const item of column.querySelectorAll(".set-item")) {
        item.hidden = !!q && !item.textContent.toLowerCase().includes(q);
      }
      for (const group of column.querySelectorAll(".set-group")) {
        group.hidden = !!q && !group.querySelector(".set-item:not([hidden])");
      }
      for (const block of column.querySelectorAll(".settings-block")) {
        block.hidden = !!q && !block.querySelector(".set-item:not([hidden])");
      }
    });
  });

  function onSearchKey(event) {
    if (event.key === "Escape" && query) {
      event.preventDefault();
      event.stopPropagation();
      query = "";
    }
  }

  const active = $derived(TABS.find((tab) => tab.id === current));
  const Pane = $derived(active.component);
  const close = () => (ui.settingsOpen = false);

  function cycleTabs(event) {
    if (event.key !== "Tab" || event.target === searchInput) return;
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
    <header class="settings-head">
      <h2 class="settings-title">{t("settings.title")}</h2>
      <button class="verdant-close" aria-label={t("reading.close")} onclick={close}><Icon name="x" size={16} /></button>
    </header>
    <div class="settings-body">
      <aside class="settings-side">
        <label class="settings-search">
          <Icon name="search" size={14} />
          <input
            bind:this={searchInput}
            bind:value={query}
            onkeydown={onSearchKey}
            type="text"
            spellcheck="false"
            placeholder={t("settings.search")}
            aria-label={t("settings.search")}
          />
        </label>
        <nav class="settings-tabs" role="tablist" aria-orientation="vertical">
          {#each TABS as tab, index (tab.id)}
            <button
              role="tab"
              aria-selected={!searching && current === tab.id}
              class={["settings-tab", !searching && current === tab.id && "active"]}
              bind:this={tabButtons[index]}
              onclick={() => { query = ""; current = tab.id; }}
            >
              <Icon name={tab.icon} size={15} />
              <span class="settings-tab-label">{t(tab.label)}</span>
            </button>
          {/each}
        </nav>
      </aside>
      <section class="settings-pane">
        <div class="settings-column" bind:this={column}>
          {#if searching}
            {#each TABS as tab (tab.id)}
              {@const Block = tab.component}
              <div class="settings-block"><Block /></div>
            {/each}
          {:else}
            <Pane />
          {/if}
        </div>
      </section>
    </div>
  </div>
</Overlay>
