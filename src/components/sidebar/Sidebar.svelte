<script>
  import { tick } from "svelte";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { compose } from "../../state/compose.svelte.js";
  import { mail } from "../../state/mail.svelte.js";
  import { prefs, setSidebarCollapsed } from "../../state/prefs.svelte.js";
  import { session } from "../../state/session.svelte.js";
  import { smart } from "../../state/smartInbox.svelte.js";
  import { ui } from "../../state/ui.svelte.js";
  import Icon from "../ui/Icon.svelte";
  import SmartCategories from "./SmartCategories.svelte";

  const HOVER_OUT_DELAY_MS = 300;
  const MAILBOXES = [
    { id: "STARRED", icon: "star", label: "sidebar.starred", count: "starred_total" },
    { id: "ARCHIVE", icon: "archive", label: "sidebar.archive", count: "archive_total" },
    { id: "SENT", icon: "send", label: "sidebar.sent", count: "sent_total" },
    { id: "DRAFT", icon: "file-text", label: "sidebar.drafts", count: "drafts_total" },
    { id: "TRASH", icon: "trash", label: "sidebar.trash", count: "trash_total" },
  ];

  let sidebar;
  let float = $state({ on: false, expanded: false, exiting: false });
  let hoverOutTimer = null;
  let stopWaitingForSlide = null;

  const collapseLabel = $derived(t(prefs.sidebarCollapsed ? "sidebar.expand" : "sidebar.collapse"));
  const smartLabel = $derived(t(smart.expanded ? "smart.collapse" : "smart.expand"));

  function cancelHoverOut() {
    clearTimeout(hoverOutTimer);
    stopWaitingForSlide?.();
    stopWaitingForSlide = null;
  }

  function toggleCollapsed() {
    cancelHoverOut();
    float = { on: false, expanded: false, exiting: false };
    setSidebarCollapsed(!prefs.sidebarCollapsed);
  }

  async function slideOut() {
    if (!prefs.sidebarCollapsed) return;
    cancelHoverOut();
    if (!float.on) {
      float.on = true;
      await tick();
      void sidebar.offsetWidth;
    }
    float.expanded = true;
  }

  function slideBackSoon() {
    if (!prefs.sidebarCollapsed || !float.on) return;
    hoverOutTimer = setTimeout(() => {
      if (!float.expanded) return;
      float.expanded = false;

      const settle = async (event) => {
        if (event.target !== sidebar || event.propertyName !== "transform") return;
        stopWaitingForSlide();
        stopWaitingForSlide = null;
        float.exiting = true;
        await tick();
        void sidebar.offsetWidth;
        float.on = false;
        await tick();
        void sidebar.offsetWidth;
        float.exiting = false;
      };
      stopWaitingForSlide = () => sidebar.removeEventListener("transitionend", settle);
      sidebar.addEventListener("transitionend", settle);
    }, HOVER_OUT_DELAY_MS);
  }
</script>

<aside
  bind:this={sidebar}
  class={["sidebar", float.on && "sidebar-float", float.expanded && "sidebar-float-expanded", float.exiting && "sidebar-float-exit"]}
  onmouseenter={slideOut}
  onmouseleave={slideBackSoon}
>
  <div class="sidebar-header">
    <div class="sidebar-menu-label">{t("sidebar.mailboxes")}</div>
    <button class="sidebar-collapse-btn" title={collapseLabel} aria-label={collapseLabel} onclick={toggleCollapsed}>
      <Icon name={prefs.sidebarCollapsed ? "sidebar-expand" : "sidebar-collapse"} />
    </button>
  </div>

  <div class="sidebar-section">
    <div
      class={["nav-item", mail.mailbox === "INBOX" && "active"]}
      role="button"
      tabindex="0"
      onclick={() => mail.open("INBOX")}
      onkeydown={(event) => event.key === "Enter" && mail.open("INBOX")}
    >
      <Icon name="inbox" />
      <span class="nav-text">{t("sidebar.inbox")}</span>
      {#if mail.counts.inbox_unread > 0}
        <span class="nav-badge">{mail.counts.inbox_unread}</span>
      {/if}
      <button
        class="inbox-expand-btn"
        title={smartLabel}
        aria-label={smartLabel}
        onclick={(event) => {
          event.stopPropagation();
          smart.setExpanded(!smart.expanded);
        }}
      >
        <Icon name={smart.expanded ? "chevron-up" : "chevron-down"} />
      </button>
    </div>

    {#if smart.expanded}
      {#if smart.sorted}
        <SmartCategories />
      {:else}
        <button class="smart-organize-btn" onclick={() => (ui.smartSetup = { resort: false })}>
          <Icon name="sparkles" /><span>{t("smart.organize")}</span><Icon name="chevron-right" />
        </button>
      {/if}
    {/if}

    {#each MAILBOXES as box (box.id)}
      <div
        class={["nav-item", mail.mailbox === box.id && "active"]}
        role="button"
        tabindex="0"
        onclick={() => mail.open(box.id)}
        onkeydown={(event) => event.key === "Enter" && mail.open(box.id)}
      >
        <Icon name={box.icon} />
        <span class="nav-text">{t(box.label)}</span>
        {#if mail.counts[box.count] > 0}
          <span class="nav-badge subtle">{mail.counts[box.count]}</span>
        {/if}
      </div>
    {/each}
  </div>

  <div class="compose-wrap">
    <button class="compose-btn" onclick={() => compose.openNew()}>
      <Icon name="plus" />
      <span class="compose-btn-label">{t("sidebar.compose")}</span>
    </button>
  </div>

  <div class="sidebar-footer">
    <div
      class="user-row"
      role="button"
      tabindex="0"
      onclick={() => (ui.accountPopoverOpen = true)}
      onkeydown={(event) => event.key === "Enter" && (ui.accountPopoverOpen = true)}
    >
      <div class="avatar">{session.profile.initials || "?"}</div>
      <div class="user-info">
        <div class="user-name">{session.profile.name || t("app.version_loading")}</div>
        <div class="user-email">{session.profile.email}</div>
      </div>
    </div>
  </div>
</aside>
