<script>
  import { dust } from "../../lib/dust.js";
  import { formatListDate, formatParticipants, sanitizeUnicodeNoise } from "../../lib/format.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { findVerificationCode } from "../../lib/verificationCode.js";
  import { mail } from "../../state/mail.svelte.js";
  import { selection } from "../../state/selection.svelte.js";
  import Avatar from "../ui/Avatar.svelte";
  import Icon from "../ui/Icon.svelte";
  import CodeChip from "./CodeChip.svelte";

  const STAGGER_MS = 40;
  const MAX_STAGGER_MS = 1200;

  let { row, index } = $props();

  const item = $derived(row.thread ?? row.email);
  const active = $derived(
    row.thread ? row.key === mail.selectedThreadId : row.key === mail.selectedEmail?.id,
  );
  const sender = $derived(
    row.thread
      ? formatParticipants(row.thread.participants)
      : sanitizeUnicodeNoise(row.email.sender || t("app.unknown_sender")),
  );
  const avatarSender = $derived(row.thread ? (row.thread.participants || "").split(",")[0] : row.email.sender || "");
  const code = $derived(
    row.thread
      ? mail.codeFor(row.thread)
      : findVerificationCode({ subject: row.email.subject, snippet: row.email.snippet, body: row.email.body_html }),
  );
</script>

<div
  class={["email-item", !item.is_read && "unread", active && "active", selection.has(row.key) && "selected"]}
  role="option"
  aria-selected={active}
  data-key={row.key}
  style:animation-delay={mail.animate ? `${Math.min(index * STAGGER_MS, MAX_STAGGER_MS)}ms` : null}
  out:dust={{ removed: () => mail.dusting.has(row.key) }}
>
  <span class="email-checkbox"><Icon name="check" size={12} /></span>
  {#if !item.is_read}
    <div class="unread-dot"></div>
  {/if}
  {#if item.starred}
    <span class={["star-badge", !mail.animate && "pop"]}><Icon name="star-filled" size={18} /></span>
  {/if}
  <div class="email-item-main">
    <Avatar sender={avatarSender} mailbox={row.thread ? "INBOX" : row.email.mailbox || ""} />
    <div class="email-item-inner">
      <div class="email-top">
        <span class="email-sender">
          {sender}{#if row.thread?.message_count > 1}<span class="thread-count">{row.thread.message_count}</span>{/if}
        </span>
        <span class="email-time">{formatListDate(row.thread ? row.thread.latest_date : row.email.date)}</span>
      </div>
      <div class="email-subject">{sanitizeUnicodeNoise(item.subject || t("app.no_subject"))}</div>
      <div class="email-preview">{sanitizeUnicodeNoise(item.snippet || "")}</div>
      {#if code}
        <CodeChip {code} animate={mail.animate} />
      {/if}
    </div>
  </div>
</div>
