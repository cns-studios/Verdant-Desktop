<script>
  import { formatListDate, formatReadingDate, htmlPreview, sanitizeUnicodeNoise, senderName } from "../../lib/format.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { findVerificationCode } from "../../lib/verificationCode.js";
  import { compose } from "../../state/compose.svelte.js";
  import Avatar from "../ui/Avatar.svelte";
  import Icon from "../ui/Icon.svelte";
  import Attachments from "./Attachments.svelte";
  import CodeCard from "./CodeCard.svelte";
  import EmailBody from "./EmailBody.svelte";
  import RecipientsLine from "./RecipientsLine.svelte";

  let { message, expanded, ontoggle } = $props();

  const sender = $derived(senderName(message.sender || t("app.unknown_sender")));
  const preview = $derived(sanitizeUnicodeNoise(message.snippet || "") || htmlPreview(message.body_html));
  const code = $derived(
    expanded ? findVerificationCode({ subject: message.subject, snippet: message.snippet, body: message.body_html }) : null,
  );

  function onHeaderKey(event) {
    if (event.target !== event.currentTarget || (event.key !== "Enter" && event.key !== " ")) return;
    event.preventDefault();
    ontoggle();
  }
</script>

<div class={["thread-bubble", expanded ? "expanded" : "collapsed"]}>
  <div class="thread-bubble-header" role="button" tabindex="0" aria-expanded={expanded} onclick={ontoggle} onkeydown={onHeaderKey}>
    <Avatar class="thread-bubble-avatar" sender={message.sender || ""} mailbox="INBOX" />
    {#if expanded}
      <div class="thread-bubble-meta-expanded">
        <span class="thread-bubble-sender">{sender}</span>
        <RecipientsLine
          email={message}
          class="thread-bubble-to"
          summary={t("reading.to_x", { name: sanitizeUnicodeNoise(message.to_recipients || t("reading.to_me")) })}
        />
      </div>
      <span class="thread-bubble-date">{formatReadingDate(message.date)}</span>
    {:else}
      <div class="thread-bubble-meta-collapsed">
        <span class="thread-bubble-sender">{sender}</span>
        <span class="thread-bubble-preview">{preview}</span>
      </div>
      <span class="thread-bubble-date">{formatListDate(message.date)}</span>
      {#if message.has_attachments}
        <span class="thread-bubble-attach-icon" title={t("thread.has_attachment")}><Icon name="paperclip" size={14} /></span>
      {/if}
    {/if}
  </div>

  {#if expanded}
    <div class="thread-bubble-body">
      {#if code}
        <CodeCard {code} />
      {/if}
      <EmailBody email={message} class="thread-bubble-content email-body-text" />
      <Attachments email={message} compact />
    </div>
    <div class="thread-bubble-actions">
      <button class="thread-reply-btn" onclick={() => compose.openReply(message)}>
        <Icon name="arrow-back-up" size={15} />
        {t("thread.reply")}
      </button>
      <button class="thread-reply-btn" onclick={() => compose.openForward(message)}>
        <Icon name="arrow-forward-up" size={15} />
        {t("thread.forward")}
      </button>
    </div>
  {/if}
</div>
