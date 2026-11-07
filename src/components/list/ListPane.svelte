<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import { mail } from "../../state/mail.svelte.js";
  import { prefs } from "../../state/prefs.svelte.js";
  import { selection } from "../../state/selection.svelte.js";
  import { ui } from "../../state/ui.svelte.js";
  import EmptyState from "../ui/EmptyState.svelte";
  import Icon from "../ui/Icon.svelte";
  import BulkBar from "./BulkBar.svelte";
  import MailRow from "./MailRow.svelte";

  const FILTERS = [
    { id: "Important", label: "list.filter.important" },
    { id: "All", label: "list.filter.all" },
    { id: "Unread", label: "list.filter.unread" },
    { id: "Attachments", label: "list.filter.attachments" },
  ];
  const EMPTY_ICONS = { INBOX: "inbox", STARRED: "star", ARCHIVE: "archive", SENT: "send", DRAFT: "file-text", TRASH: "trash" };
  const DRAG_THRESHOLD = 6;
  const LONG_PRESS_MS = 450;
  const FETCH_MORE_DISTANCE = 80;

  let list;
  let searchInput;
  let press = null;
  let dragging = false;
  let suppressClick = false;

  const viewKey = $derived(`${mail.mailbox}|${mail.deepResults ? "search" : "local"}`);
  const paneStyle = $derived(
    prefs.listPaneWidth > 0
      ? `width:${prefs.listPaneWidth}px;min-width:${prefs.listPaneWidth}px;flex:0 0 ${prefs.listPaneWidth}px`
      : null,
  );

  const empty = $derived.by(() => {
    const query = mail.search.trim();
    if (query) return { icon: "search", title: t("empty.search.title"), body: t("empty.search.body", { q: query }) };
    if (mail.filter === "Unread" || mail.filter === "Attachments") {
      return {
        icon: "list",
        title: t("empty.filter.title"),
        body: t("empty.filter.body"),
        action: t("empty.filter.action"),
        onaction: () => (mail.filter = "All"),
      };
    }
    if (mail.categorySlug) return { icon: "tag", title: t("empty.category.title"), body: t("empty.category.body") };
    const box = EMPTY_ICONS[mail.mailbox] ? mail.mailbox : "INBOX";
    return { icon: EMPTY_ICONS[box], title: t(`empty.${box}.title`), body: t(`empty.${box}.body`) };
  });

  $effect(() => {
    if (ui.focusSearch) searchInput?.focus();
  });

  const rowKeyAt = (target) => (target instanceof Element ? target.closest(".email-item")?.dataset.key : null) ?? null;

  function onMouseDown(event) {
    if (event.button !== 0) return;
    const key = rowKeyAt(event.target);
    press = { x: event.clientX, y: event.clientY, timer: null };
    dragging = false;
    if (!key) return;
    press.timer = setTimeout(() => {
      if (dragging) return;
      suppressClick = true;
      selection.enter(key);
    }, LONG_PRESS_MS);
  }

  function onMouseMove(event) {
    if (!press || event.buttons === 0) return;
    const under = document.elementFromPoint(event.clientX, event.clientY);
    if (dragging) {
      const key = rowKeyAt(under);
      if (key) selection.add(key);
      return;
    }
    if (!under || !list.contains(under)) return;
    if (Math.abs(event.clientX - press.x) > DRAG_THRESHOLD || Math.abs(event.clientY - press.y) > DRAG_THRESHOLD) {
      clearTimeout(press.timer);
      dragging = true;
      selection.enter();
    }
  }

  function onMouseUp() {
    clearTimeout(press?.timer);
    press = null;
    if (!dragging) return;
    dragging = false;
    suppressClick = true;
    setTimeout(() => (suppressClick = false), 0);
  }

  function onClick(event) {
    if (suppressClick) {
      suppressClick = false;
      return;
    }
    const row = mail.rowByKey(rowKeyAt(event.target));
    if (!row) return;
    const onAvatar = event.target instanceof Element && event.target.closest(".avatar-slot");
    if (selection.active) selection.toggle(row.key);
    else if (onAvatar) selection.enter(row.key);
    else mail.selectRow(row);
  }

  function onContextMenu(event) {
    const row = mail.rowByKey(rowKeyAt(event.target));
    if (!row) return;
    event.preventDefault();
    ui.categoryPopup = null;
    ui.contextMenu = { x: event.clientX, y: event.clientY, row };
  }

  function onScroll() {
    if (list.scrollHeight - list.scrollTop - list.clientHeight < FETCH_MORE_DISTANCE) mail.fetchMore();
  }
</script>

<svelte:document onmousemove={onMouseMove} onmouseup={onMouseUp} />

<div class="email-list-pane" style={paneStyle}>
  <div class="list-header">
    <div class="search-row">
      <div class="search-bar has-deep-btn">
        <Icon name="search" />
        <input
          type="text"
          placeholder={t("list.search.placeholder")}
          bind:this={searchInput}
          value={mail.search}
          oninput={(event) => mail.setSearch(event.currentTarget.value)}
        />
      </div>
      <button class="deep-search-btn" disabled={mail.deepSearching} onclick={() => mail.deepSearch()}>
        {t(mail.deepSearching ? "list.search.searching" : "list.search.deep")}
      </button>
    </div>
    <div class={["filter-bar", selection.active && "bulk-mode"]}>
      <div class="filter-chips">
        {#each FILTERS as filter (filter.id)}
          <div
            class={["chip", mail.filter === filter.id && "active"]}
            role="button"
            tabindex="0"
            onclick={() => (mail.filter = filter.id)}
            onkeydown={(event) => event.key === "Enter" && (mail.filter = filter.id)}
          >
            {t(filter.label)}
          </div>
        {/each}
      </div>
      <span class="list-count">{t("list.count", { n: mail.rows.length })}</span>
      <BulkBar />
    </div>
  </div>

  <div class={["list-sync-bar", mail.activeSyncs > 0 && "visible"]}>
    <div class="list-sync-bar-inner"></div>
  </div>

  <div
    id="email-list"
    class={["email-list", !mail.animate && "suppress-anim", (selection.active || selection.count > 0) && "multi-select"]}
    role="listbox"
    tabindex="-1"
    bind:this={list}
    onmousedown={onMouseDown}
    onclick={onClick}
    oncontextmenu={onContextMenu}
    onscroll={onScroll}
  >
    {#if mail.loading}
      <div class="list-loading"><div class="list-spinner"></div></div>
    {:else if !mail.rows.length}
      <EmptyState {...empty} />
    {:else}
      {#key viewKey}
        {#each mail.rows as row, index (row.key)}
          <MailRow {row} {index} />
        {/each}
      {/key}
    {/if}
  </div>

  {#if mail.fetchNote}
    <div class="list-fetch-indicator">{mail.fetchNote}</div>
  {/if}
</div>
