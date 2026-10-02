<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import { compose } from "../../state/compose.svelte.js";
  import { mail } from "../../state/mail.svelte.js";
  import { smart } from "../../state/smartInbox.svelte.js";
  import { openCategoryPopup, ui } from "../../state/ui.svelte.js";
  import Icon from "../ui/Icon.svelte";

  let menuOpen = $state(false);
  let moreButton;

  const row = $derived(mail.selectedRow);
  const email = $derived(mail.selectedEmail);
  const box = $derived(`${email?.mailbox || ""} ${mail.mailbox}`.toUpperCase());
  const isDraft = $derived(!!email && box.includes("DRAFT"));
  const isTrash = $derived(!!email && box.includes("TRASH"));
  const isSent = $derived(!!email && box.includes("SENT"));
  const isArchive = $derived(!!email && box.includes("ARCHIVE"));

  const unsubscribeTarget = $derived(email ?? mail.conversation.messages.at(-1) ?? null);
  const canUnsubscribe = $derived(
    !!unsubscribeTarget?.list_unsubscribe?.trim() && !isDraft && !isSent && !isTrash,
  );

  const move = $derived.by(() => {
    if (isTrash) return { icon: "rotate", label: "reading.restore", run: () => mail.restore([row]) };
    if (isArchive) return { icon: "inbox", label: "reading.move_to_inbox", run: () => mail.moveToInbox([row]) };
    if (isDraft || isSent) return null;
    return { icon: "archive", label: "reading.archive", run: () => mail.archive([row]) };
  });

  const menu = $derived.by(() => {
    const entries = [];
    if (smart.sorted) entries.push({ label: "reading.move_to_category", run: chooseCategory });
    entries.push(
      { label: "reading.mark_read", run: () => mail.markRead([row], true) },
      { label: "reading.mark_unread_action", run: () => mail.markRead([row], false) },
      { label: "reading.toggle_star", run: () => mail.toggleStar([row]) },
    );
    if (isDraft) {
      entries.push(
        { label: "reading.edit_draft", run: () => compose.openDraft(email) },
        { label: "reading.send_draft", run: () => mail.sendDraft(email) },
      );
    }
    if (isTrash) {
      entries.push(
        { label: "reading.restore", run: () => mail.restore([row]) },
        { label: "reading.permanent_delete", run: () => mail.trash([row]) },
      );
    }
    return entries;
  });

  function chooseCategory() {
    const rect = moreButton.getBoundingClientRect();
    const target = row;
    openCategoryPopup(rect.left, rect.bottom + 4, (category) => mail.moveToCategory([target], category));
  }

  const act = (action) => () => row && action();
</script>

<svelte:document onclick={() => (menuOpen = false)} />

<div class="reading-actions">
  {#if move}
    <button class="icon-btn" title={t(move.label)} onclick={act(move.run)}><Icon name={move.icon} /></button>
  {/if}
  <button
    class="icon-btn danger"
    title={t(isTrash ? "reading.permanent_delete" : "reading.delete")}
    onclick={act(() => mail.trash([row]))}
  >
    <Icon name={isTrash ? "trash-x" : "trash"} />
  </button>
  {#if !isSent}
    <button class="icon-btn" title={t("reading.mark_unread")} onclick={act(() => mail.markRead([row], false))}>
      <Icon name="mail" />
    </button>
  {/if}
  {#if !isDraft && !isSent && !isTrash}
    <button
      class={["icon-btn", (row?.thread ?? row?.email)?.starred && "active"]}
      title={t("reading.star")}
      onclick={act(() => mail.toggleStar([row]))}
    >
      <Icon name="star" />
    </button>
  {/if}
  {#if canUnsubscribe}
    <button
      class={["unsubscribe-btn", unsubscribeTarget.unsubscribed && "unsubscribed"]}
      disabled={unsubscribeTarget.unsubscribed}
      onclick={() => (ui.unsubscribe = unsubscribeTarget)}
    >
      {t(unsubscribeTarget.unsubscribed ? "reading.unsubscribed" : "reading.unsubscribe")}
    </button>
  {/if}
  <div class="action-menu-anchor">
    <button
      class="icon-btn"
      title={t("reading.more")}
      bind:this={moreButton}
      onclick={(event) => {
        event.stopPropagation();
        menuOpen = !menuOpen && !!row;
      }}
    >
      <Icon name="dots" />
    </button>
    {#if menuOpen}
      <div class="action-menu" role="menu">
        {#each menu as entry (entry.label)}
          <button role="menuitem" onclick={entry.run}>{t(entry.label)}</button>
        {/each}
      </div>
    {/if}
  </div>
  <button class="icon-btn" title={t("reading.close")} aria-label={t("reading.close")} onclick={() => mail.clearSelection()}>
    <Icon name="x" />
  </button>
</div>
