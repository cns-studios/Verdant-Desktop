<script>
  import { formatReadingDate, sanitizeUnicodeNoise } from "../../lib/format.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { mail } from "../../state/mail.svelte.js";
  import Avatar from "../ui/Avatar.svelte";
  import MessageView from "./MessageView.svelte";
  import ReadingActions from "./ReadingActions.svelte";
  import RecipientsLine from "./RecipientsLine.svelte";
  import ThreadView from "./ThreadView.svelte";

  const email = $derived(mail.selectedEmail);
  const subject = $derived(sanitizeUnicodeNoise((mail.selectedThread ?? email)?.subject || t("app.no_subject")));
</script>

<div class="reading-pane">
  <div class="reading-header">
    <ReadingActions />
    <div class="reading-subject">{mail.hasSelection ? subject : ""}</div>
    {#if email}
      <div class="reading-meta">
        <Avatar class="meta-avatar" sender={email.sender || ""} mailbox={email.mailbox || ""} />
        <div class="meta-info">
          <div class="meta-from">{sanitizeUnicodeNoise(email.sender || t("app.unknown_sender"))}</div>
          {#key email.id}
            <RecipientsLine {email} class="meta-to" />
          {/key}
        </div>
        <div class="meta-date">{formatReadingDate(email.date || "")}</div>
      </div>
    {/if}
  </div>

  <div class="reading-body">
    {#if email}
      {#key email.id}
        <MessageView {email} />
      {/key}
    {:else if mail.conversation.loading}
      <div class="thread-loading">{t("toast.fetching")}</div>
    {:else if mail.conversation.error}
      <div class="thread-loading" style:color="#8a3b3b">{mail.conversation.error}</div>
    {:else if mail.selectedThreadId}
      {#key mail.selectedThreadId}
        <ThreadView />
      {/key}
    {/if}
  </div>
</div>
