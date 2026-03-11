<script lang="ts">
  import { activeMessage, loadingBody } from "../stores/emails";
  import { composeOpen } from "../stores/ui";
  import { senderInitials } from "../api";

  let replyTo = "";
  let replySubject = "";
  let replyInReplyTo = "";
  let replyReferences = "";

  function openReply() {
    if (!$activeMessage) return;
    replyTo = $activeMessage.sender_email;
    replySubject = $activeMessage.subject.startsWith("Re:")
      ? $activeMessage.subject
      : `Re: ${$activeMessage.subject}`;
    replyInReplyTo = $activeMessage.message_id ?? "";
    replyReferences = [
      $activeMessage.references_hdr ?? "",
      $activeMessage.message_id ?? "",
    ]
      .filter(Boolean)
      .join(" ");
    composeOpen.set(true);
    // Pass reply context via a custom event bubbled up to App
    window.dispatchEvent(
      new CustomEvent("verdant:reply", {
        detail: {
          to: replyTo,
          subject: replySubject,
          inReplyTo: replyInReplyTo,
          references: replyReferences,
        },
      })
    );
  }

  $: msg = $activeMessage;
</script>

<div class="reading-pane">
  {#if !msg}
    <div class="empty-state">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" style="width:48px;height:48px;opacity:0.15">
        <path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z"/>
        <polyline points="22,6 12,13 2,6"/>
      </svg>
      <p>Select a message to read</p>
    </div>
  {:else}
    <div class="reading-header">
      <div class="reading-actions">
        <button class="icon-btn" title="Archive">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="21 8 21 21 3 21 3 8"/><rect x="1" y="3" width="22" height="5"/><line x1="10" y1="12" x2="14" y2="12"/></svg>
        </button>
        <button class="icon-btn" title="Delete">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14H6L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/><path d="M9 6V4h6v2"/></svg>
        </button>
        <button class="icon-btn" title="Mark unread">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z"/><polyline points="22,6 12,13 2,6"/></svg>
        </button>
        <button class="icon-btn" title="Star">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/></svg>
        </button>
        <button class="icon-btn" title="More" style="margin-left:auto">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="1"/><circle cx="19" cy="12" r="1"/><circle cx="5" cy="12" r="1"/></svg>
        </button>
      </div>

      <div class="reading-subject">{msg.subject || "(no subject)"}</div>

      <div class="reading-meta">
        <div class="meta-avatar">{senderInitials(msg)}</div>
        <div class="meta-info">
          <div class="meta-from">
            {msg.sender_name ? `${msg.sender_name} \u003c${msg.sender_email}\u003e` : msg.sender_email}
          </div>
          <div class="meta-to">to me</div>
        </div>
        <div class="meta-date">{msg.date_str}</div>
      </div>
    </div>

    <div class="reading-body">
      {#if $loadingBody}
        <div class="body-loading">Loading message…</div>
      {:else if msg.body_html}
        <!-- Sandboxed iframe for HTML email rendering — no scripts allowed -->
        <iframe
          title="Email body"
          class="email-iframe"
          sandbox="allow-same-origin"
          srcdoc={msg.body_html}
        ></iframe>
      {:else if msg.body_text}
        <div class="email-body-text">
          <pre class="plain-text">{msg.body_text}</pre>
        </div>
      {:else}
        <div class="email-body-text">
          <p>{msg.preview}</p>
        </div>
      {/if}
    </div>

    <div class="reply-bar">
      <div class="reply-box" on:click={openReply} role="button" tabindex="0" on:keydown={e => e.key === "Enter" && openReply()}>
        <span>Reply to {msg.sender_name || msg.sender_email}…</span>
        <button class="reply-send-btn" on:click|stopPropagation={openReply}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/></svg>
          Reply
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .empty-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    color: var(--text-muted);
    font-size: 13px;
  }

  .body-loading {
    padding: 32px;
    color: var(--text-muted);
    font-size: 13px;
  }

  .email-iframe {
    width: 100%;
    height: 100%;
    border: none;
    background: white;
  }

  .plain-text {
    font-family: 'DM Sans', sans-serif;
    font-size: 14px;
    line-height: 1.7;
    white-space: pre-wrap;
    word-break: break-word;
    color: var(--text-mid);
  }
</style>
