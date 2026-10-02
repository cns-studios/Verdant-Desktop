<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import { mail } from "../../state/mail.svelte.js";
  import { smart } from "../../state/smartInbox.svelte.js";
  import { openCategoryPopup, ui } from "../../state/ui.svelte.js";
  import Icon from "../ui/Icon.svelte";
  import FloatingMenu from "./FloatingMenu.svelte";

  let { x, y, row } = $props();

  const items = $derived.by(() => {
    const rows = [row];
    const box = mail.mailbox.toUpperCase();
    const isTrash = box.includes("TRASH");
    const isSent = box.includes("SENT");
    const isDraft = box.includes("DRAFT");
    const movable = !isDraft && !isSent && !isTrash;
    const list = [];

    if (isTrash) list.push({ icon: "rotate", label: "reading.restore", run: () => mail.restore(rows) });
    else if (box.includes("ARCHIVE")) list.push({ icon: "inbox", label: "reading.move_to_inbox", run: () => mail.moveToInbox(rows) });
    else if (!isDraft && !isSent) list.push({ icon: "archive", label: "reading.archive", run: () => mail.archive(rows) });

    list.push({
      icon: "trash",
      label: isTrash ? "reading.permanent_delete" : "reading.delete",
      danger: true,
      run: () => mail.trash(rows),
    });
    if (!isSent) list.push({ icon: "mail", label: "reading.mark_unread", run: () => mail.markRead(rows, false) });
    if (movable) {
      list.push({
        icon: "star",
        label: (row.thread ?? row.email).starred ? "reading.unstar" : "reading.star",
        run: () => mail.toggleStar(rows),
      });
    }
    if (movable && smart.sorted) {
      list.push({
        icon: "folder",
        label: "reading.move_to_category",
        run: () => openCategoryPopup(x, y, (category) => mail.moveToCategory(rows, category)),
      });
    }
    return list;
  });

  function choose(item) {
    item.run();
    ui.contextMenu = null;
  }
</script>

<FloatingMenu {x} {y} class="email-context-menu" onclose={() => (ui.contextMenu = null)}>
  {#each items as item (item.label)}
    <button class={["email-context-item", item.danger && "danger"]} role="menuitem" onclick={() => choose(item)}>
      <Icon name={item.icon} /><span>{t(item.label)}</span>
    </button>
  {/each}
</FloatingMenu>
