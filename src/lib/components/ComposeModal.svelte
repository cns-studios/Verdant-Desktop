<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { composeOpen, showNotification, syncing } from "../stores/ui";
  import { accounts, activeAccountId } from "../stores/accounts";
  import { sendEmail } from "../api";
  import type { OutgoingMessage } from "../api";

  export let initialTo = "";
  export let initialSubject = "";
  export let initialInReplyTo = "";
  export let initialReferences = "";

  let to = initialTo;
  let cc = "";
  let subject = initialSubject;
  let body = "";
  let sending = false;

  // React to reply events
  function onReplyEvent(e: Event) {
    const detail = (e as CustomEvent).detail;
    to = detail.to ?? "";
    subject = detail.subject ?? "";
    initialInReplyTo = detail.inReplyTo ?? "";
    initialReferences = detail.references ?? "";
  }

  onMount(() => {
    window.addEventListener("verdant:reply", onReplyEvent);
  });

  onDestroy(() => {
    window.removeEventListener("verdant:reply", onReplyEvent);
  });

  $: account = $accounts.find(a => a.id === $activeAccountId) ?? $accounts[0];

  async function send() {
    if (!account) {
      showNotification("No account configured.", "error");
      return;
    }
    if (!to.trim()) {
      showNotification("Please enter a recipient.", "error");
      return;
    }
    sending = true;
    try {
      const msg: OutgoingMessage = {
        from_name: account.name,
        from_email: account.email,
        to: to.split(",").map(s => s.trim()).filter(Boolean),
        cc: cc.split(",").map(s => s.trim()).filter(Boolean),
        subject: subject,
        body: body,
        in_reply_to: initialInReplyTo || undefined,
        references: initialReferences || undefined,
      };
      await sendEmail(account.id, msg);
      showNotification("Message sent.");
      close();
    } catch (err) {
      showNotification(`Failed to send: ${err}`, "error");
    } finally {
      sending = false;
    }
  }

  function close() {
    composeOpen.set(false);
    to = "";
    cc = "";
    subject = "";
    body = "";
    initialInReplyTo = "";
    initialReferences = "";
  }

  function handleOverlayClick(e: MouseEvent) {
    if ((e.target as Element).classList.contains("modal-overlay")) {
      close();
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") close();
  }
</script>

<svelte:window on:keydown={handleKeydown} />

<!-- svelte-ignore a11y-click-events-have-key-events -->
<div
  class="modal-overlay"
  class:open={$composeOpen}
  on:click={handleOverlayClick}
  role="presentation"
>
  <div class="compose-modal">
    <div class="modal-header">
      <span class="modal-title">
        {initialInReplyTo ? "Reply" : "New Message"}
      </span>
      <button class="modal-close" on:click={close}>×</button>
    </div>

    <div class="modal-fields">
      <div class="modal-field">
        <label for="compose-to">To</label>
        <input id="compose-to" type="text" placeholder="recipient@example.com" bind:value={to} />
      </div>
      <div class="modal-field">
        <label for="compose-cc">Cc</label>
        <input id="compose-cc" type="text" placeholder="Add cc…" bind:value={cc} />
      </div>
      <div class="modal-field">
        <label for="compose-subject">Re</label>
        <input id="compose-subject" type="text" placeholder="Subject" bind:value={subject} />
      </div>
    </div>

    <div class="modal-body">
      <textarea
        placeholder="Write your message…"
        bind:value={body}
        disabled={sending}
      ></textarea>
    </div>

    <div class="modal-footer">
      <div class="modal-tools">
        <button class="modal-tool" title="Attach file">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/>
          </svg>
        </button>
        <button class="modal-tool" title="Emoji">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="10"/><path d="M8 14s1.5 2 4 2 4-2 4-2"/><line x1="9" y1="9" x2="9.01" y2="9"/><line x1="15" y1="9" x2="15.01" y2="9"/>
          </svg>
        </button>
        <button class="modal-tool" title="Format text">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M4 7V4h16v3"/><path d="M9 20h6"/><path d="M12 4v16"/>
          </svg>
        </button>
      </div>
      <button class="send-btn" on:click={send} disabled={sending}>
        {#if sending}
          Sending…
        {:else}
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
            <line x1="22" y1="2" x2="11" y2="13"/>
            <polygon points="22 2 15 22 11 13 2 9 22 2"/>
          </svg>
          Send
        {/if}
      </button>
    </div>
  </div>
</div>
