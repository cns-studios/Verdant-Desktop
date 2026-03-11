<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";

  import Sidebar from "./lib/components/Sidebar.svelte";
  import EmailListPane from "./lib/components/EmailListPane.svelte";
  import ReadingPane from "./lib/components/ReadingPane.svelte";
  import ComposeModal from "./lib/components/ComposeModal.svelte";
  import AddAccountModal from "./lib/components/AddAccountModal.svelte";

  import { loadAccounts, addingAccount } from "./lib/stores/accounts";
  import { loadMessages } from "./lib/stores/emails";
  import { notification, syncing } from "./lib/stores/ui";

  onMount(async () => {
    await loadAccounts();

    await listen<{ mailbox_id: string; account_id: string; count: number }>(
      "mail://sync-done",
      async (event) => {
        const { mailbox_id } = event.payload;
        if (mailbox_id) {
          await loadMessages(mailbox_id);
        }
      }
    );
  });
</script>

<Sidebar />
<EmailListPane />
<ReadingPane />
<ComposeModal />

{#if $addingAccount}
  <AddAccountModal />
{/if}

{#if $notification}
  <div class="toast" class:toast-error={$notification.type === "error"}>
    {$notification.text}
  </div>
{/if}

{#if $syncing}
  <div class="sync-indicator">Syncing…</div>
{/if}

<style>
  .toast {
    position: fixed;
    bottom: 24px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--text);
    color: var(--white);
    padding: 10px 20px;
    border-radius: 8px;
    font-size: 13px;
    z-index: 200;
    box-shadow: var(--shadow-lg);
    animation: fadeSlideIn 0.2s ease both;
  }

  .toast-error {
    background: #b94040;
  }

  .sync-indicator {
    position: fixed;
    top: 12px;
    right: 16px;
    font-size: 11px;
    color: var(--green);
    background: var(--green-pale);
    padding: 4px 10px;
    border-radius: 20px;
    border: 1px solid var(--green-muted);
    z-index: 50;
  }
</style>