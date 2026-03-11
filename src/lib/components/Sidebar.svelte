<script lang="ts">
  import { accounts, activeAccountId, mailboxes, activeMailboxId, loadMailboxes, addingAccount } from "../stores/accounts";
  import { loadMessages } from "../stores/emails";
  import { composeOpen } from "../stores/ui";

  async function selectMailbox(id: string) {
    activeMailboxId.set(id);
    await loadMessages(id);
  }

  async function switchAccount(id: string) {
    activeAccountId.set(id);
    await loadMailboxes(id);
    const $activeMailboxId = $mailboxes.find(m => m.full_name.toLowerCase() === "inbox")?.id
      ?? $mailboxes[0]?.id;
    if ($activeMailboxId) await loadMessages($activeMailboxId);
  }

  // Map well-known folder names to icons
  function folderIcon(name: string): string {
    const n = name.toLowerCase();
    if (n.includes("inbox"))   return "inbox";
    if (n.includes("sent"))    return "send";
    if (n.includes("draft"))   return "file";
    if (n.includes("trash") || n.includes("deleted")) return "trash";
    if (n.includes("spam") || n.includes("junk"))     return "alert";
    if (n.includes("star"))    return "star";
    return "folder";
  }
</script>

<aside class="sidebar">
  <div class="sidebar-header">
    <div class="logo">
      <div class="logo-mark">
        <svg viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z"/>
          <polyline points="22,6 12,13 2,6"/>
        </svg>
      </div>
      <span class="logo-name">Verdant</span>
    </div>
    <button class="compose-btn" on:click={() => composeOpen.set(true)}>
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
      </svg>
      Compose
    </button>
  </div>

  <div class="sidebar-section">
    {#if $mailboxes.length > 0}
      <div class="section-label">Mailboxes</div>
      {#each $mailboxes as mb}
        <div
          class="nav-item"
          class:active={$activeMailboxId === mb.id}
          on:click={() => selectMailbox(mb.id)}
          on:keydown={e => e.key === "Enter" && selectMailbox(mb.id)}
          role="button"
          tabindex="0"
        >
          <!-- icon slot based on folder type -->
          {#if folderIcon(mb.name) === "inbox"}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 13V6a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2v12c0 1.1.9 2 2 2h8"/><polyline points="22,6 12,13 2,6"/></svg>
          {:else if folderIcon(mb.name) === "send"}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/></svg>
          {:else if folderIcon(mb.name) === "file"}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>
          {:else if folderIcon(mb.name) === "trash"}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14H6L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/><path d="M9 6V4h6v2"/></svg>
          {:else}
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg>
          {/if}
          {mb.name}
        </div>
      {/each}
    {:else}
      <div class="nav-item-placeholder">No account set up yet.</div>
    {/if}
  </div>

  <div class="sidebar-footer">
    <!-- Multi-account switcher -->
    {#each $accounts as acct}
      <div
        class="user-row"
        class:active-account={$activeAccountId === acct.id}
        on:click={() => switchAccount(acct.id)}
        on:keydown={e => e.key === "Enter" && switchAccount(acct.id)}
        role="button"
        tabindex="0"
      >
        <div class="avatar">{acct.name.slice(0,2).toUpperCase()}</div>
        <div class="user-info">
          <div class="user-name">{acct.name}</div>
          <div class="user-email">{acct.email}</div>
        </div>
      </div>
    {/each}
    <div
      class="user-row add-account-row"
      on:click={() => addingAccount.set(true)}
      on:keydown={e => e.key === "Enter" && addingAccount.set(true)}
      role="button"
      tabindex="0"
    >
      <div class="avatar add-icon">+</div>
      <div class="user-info">
        <div class="user-name">Add account</div>
      </div>
    </div>
  </div>
</aside>

<style>
  .nav-item-placeholder {
    font-size: 12px;
    color: var(--text-muted);
    padding: 8px 10px;
  }

  .active-account {
    background: var(--surface2);
    border-radius: 8px;
  }

  .add-account-row {
    margin-top: 4px;
    opacity: 0.7;
  }

  .add-account-row:hover {
    opacity: 1;
  }

  .add-icon {
    font-size: 18px;
    font-weight: 300;
    background: var(--surface2);
    color: var(--text-mid);
  }
</style>
