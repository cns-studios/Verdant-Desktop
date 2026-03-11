<script lang="ts">
  import { mailboxes, activeMailboxId } from "../stores/accounts";
  import {
    displayedMessages,
    activeMessageId,
    loadingMessages,
    searchQuery,
    searchMessages,
    selectMessage,
  } from "../stores/emails";
  import { activeFilter } from "../stores/ui";
  import { isUnread, formatDate, senderInitials } from "../api";
  import type { Message } from "../api";

  let searchInput = "";
  let debounceTimer: ReturnType<typeof setTimeout>;

  function onSearch() {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      searchQuery.set(searchInput);
      searchMessages(searchInput);
    }, 300);
  }

  function clearSearch() {
    searchInput = "";
    searchQuery.set("");
    searchMessages("");
  }

  $: activeMailboxName = $mailboxes.find(m => m.id === $activeMailboxId)?.name ?? "Inbox";

  $: filtered = (() => {
    let list = $displayedMessages;
    if ($activeFilter === "unread")      list = list.filter(isUnread);
    if ($activeFilter === "attachments") list = list.filter(m => m.has_attachments);
    if ($activeFilter === "flagged")     list = list.filter(m => m.flags.includes("Flagged"));
    return list;
  })();

  function isActive(msg: Message) {
    return $activeMessageId === msg.id;
  }

  function tagFromMailbox(msg: Message): string | null {
    const mb = $mailboxes.find(m => m.id === msg.mailbox_id);
    if (!mb) return null;
    const n = mb.name.toLowerCase();
    if (n.includes("work")) return "Work";
    if (n.includes("personal")) return "Personal";
    if (n.includes("finance")) return "Finance";
    return null;
  }
</script>

<div class="email-list-pane">
  <div class="list-header">
    <div class="list-title-row">
      <span class="list-title">{activeMailboxName}</span>
      <span class="list-count">{filtered.length} messages</span>
    </div>
    <div class="search-bar" class:has-value={searchInput.length > 0}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
      </svg>
      <input
        type="text"
        placeholder="Search mail…"
        bind:value={searchInput}
        on:input={onSearch}
      />
      {#if searchInput}
        <button class="clear-btn" on:click={clearSearch}>×</button>
      {/if}
    </div>
    <div class="filter-chips">
      {#each ["all", "unread", "attachments", "flagged"] as chip}
        <div
          class="chip"
          class:active={$activeFilter === chip}
          on:click={() => activeFilter.set(chip)}
          on:keydown={e => e.key === "Enter" && activeFilter.set(chip)}
          role="button"
          tabindex="0"
        >
          {chip.charAt(0).toUpperCase() + chip.slice(1)}
        </div>
      {/each}
    </div>
  </div>

  <div class="email-list">
    {#if $loadingMessages}
      <div class="list-loading">Loading…</div>
    {:else if filtered.length === 0}
      <div class="list-empty">No messages found.</div>
    {:else}
      {#each filtered as msg, i}
        <div
          class="email-item"
          class:unread={isUnread(msg)}
          class:active={isActive(msg)}
          style="animation-delay: {Math.min(i * 0.04, 0.2)}s"
          on:click={() => selectMessage(msg)}
          on:keydown={e => e.key === "Enter" && selectMessage(msg)}
          role="button"
          tabindex="0"
        >
          {#if isUnread(msg)}
            <div class="unread-dot"></div>
          {/if}
          <div class="email-item-inner">
            <div class="email-top">
              <span class="email-sender">{msg.sender_name || msg.sender_email}</span>
              <span class="email-time">{formatDate(msg.date_ts)}</span>
            </div>
            <div class="email-subject">{msg.subject || "(no subject)"}</div>
            <div class="email-preview">{msg.preview}</div>
            {#if tagFromMailbox(msg)}
              <div class="email-tags">
                <span class="email-tag">{tagFromMailbox(msg)}</span>
              </div>
            {/if}
          </div>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .search-bar.has-value {
    border-color: var(--green-light);
  }

  .clear-btn {
    border: none;
    background: none;
    cursor: pointer;
    color: var(--text-muted);
    font-size: 16px;
    line-height: 1;
    padding: 0 2px;
  }

  .clear-btn:hover {
    color: var(--text);
  }

  .list-loading,
  .list-empty {
    padding: 32px 18px;
    text-align: center;
    font-size: 13px;
    color: var(--text-muted);
  }
</style>
