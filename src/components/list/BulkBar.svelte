<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import { mail } from "../../state/mail.svelte.js";
  import { selection } from "../../state/selection.svelte.js";
  import { session } from "../../state/session.svelte.js";
  import { smart } from "../../state/smartInbox.svelte.js";
  import { openCategoryPopup } from "../../state/ui.svelte.js";
  import Icon from "../ui/Icon.svelte";

  const visibleKeys = $derived(mail.rows.map((row) => row.key));
  const selectedRows = $derived(mail.rows.filter((row) => selection.has(row.key)));
  const masterState = $derived(selection.masterState(visibleKeys));
  const allStarred = $derived(selectedRows.length > 0 && selectedRows.every((row) => (row.thread ?? row.email).starred));

  const box = $derived(mail.mailbox.toUpperCase());
  const isTrash = $derived(box.includes("TRASH"));
  const isSent = $derived(box.includes("SENT"));
  const isDraft = $derived(box.includes("DRAFT"));
  const isArchive = $derived(box.includes("ARCHIVE"));
  const movable = $derived(!isDraft && !isSent && !isTrash);
  const anyUnread = $derived(selectedRows.some((row) => !(row.thread ?? row.email).is_read));

  const move = $derived.by(() => {
    if (isTrash) return { icon: "rotate", label: "reading.restore", run: (rows) => mail.restore(rows) };
    if (isArchive) return { icon: "inbox", label: "reading.move_to_inbox", run: (rows) => mail.moveToInbox(rows) };
    if (isDraft || isSent) return null;
    return { icon: "archive", label: "bulk.archive", run: (rows) => mail.archive(rows) };
  });

  async function run(action) {
    if (!selectedRows.length) return;
    const rows = selectedRows;
    selection.exit();
    await action(rows);
  }

  function chooseCategory(event) {
    if (!selectedRows.length) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const rows = selectedRows;
    openCategoryPopup(rect.left, rect.bottom + 4, (category) => {
      selection.exit();
      mail.moveToCategory(rows, category);
    });
  }
</script>

<div class={["bulk-bar", selectedRows.length === 0 && "empty"]}>
  <button
    class={["bulk-master", `state-${masterState}`]}
    title={t(masterState === "all" ? "bulk.deselect_all" : "bulk.select_all")}
    onclick={() => (masterState === "all" ? selection.clear() : selection.selectAll(visibleKeys))}
  >
    <span class="bulk-master-box">
      <span class="bulk-ic-check"><Icon name="check" size={12} /></span>
      <span class="bulk-ic-dash"><Icon name="minus" size={12} /></span>
    </span>
  </button>
  <div class="bulk-actions">
    {#if move}
      <button class="bulk-action" title={t(move.label)} onclick={() => run(move.run)}>
        <Icon name={move.icon} size={14} /><span>{t(move.label)}</span>
      </button>
    {/if}
    {#if !isTrash || session.canDeleteForever}
      <button
        class="bulk-action danger"
        title={t(isTrash ? "reading.permanent_delete" : "bulk.delete")}
        onclick={() => run((rows) => mail.trash(rows))}
      >
        <Icon name={isTrash ? "trash-x" : "trash"} size={14} /><span>{t(isTrash ? "reading.permanent_delete" : "bulk.delete")}</span>
      </button>
    {/if}
    {#if !isSent}
      <button
        class="bulk-action"
        title={t(anyUnread ? "bulk.mark_read" : "bulk.mark_unread")}
        onclick={() => selectedRows.length && mail.markRead(selectedRows, anyUnread)}
      >
        <Icon name={anyUnread ? "check" : "mail"} size={14} /><span>{t(anyUnread ? "bulk.mark_read" : "bulk.mark_unread")}</span>
      </button>
    {/if}
    {#if movable}
      <button
        class={["bulk-action", allStarred && "active"]}
        title={t("bulk.star")}
        onclick={() => selectedRows.length && mail.toggleStar(selectedRows)}
      >
        <Icon name="star" size={14} /><span>{t("bulk.star")}</span>
      </button>
    {/if}
    {#if movable && smart.sorted}
      <button class="bulk-action smart-move-action" title={t("bulk.move")} onclick={chooseCategory}>
        <Icon name="folder" size={14} /><span>{t("bulk.move")}</span>
      </button>
    {/if}
    <button class="bulk-action close" title={t("bulk.close")} onclick={() => selection.exit()}>
      <Icon name="x" size={14} /><span>{t("bulk.close")}</span>
    </button>
  </div>
</div>
