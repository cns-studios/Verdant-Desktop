<script>
  import { getEmail } from "../../lib/api.js";
  import { formatListDate, sanitizeUnicodeNoise } from "../../lib/format.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { mail } from "../../state/mail.svelte.js";
  import { prefs } from "../../state/prefs.svelte.js";
  import { subscriptions as subs } from "../../state/subscriptions.svelte.js";
  import { showToast } from "../../state/ui.svelte.js";
  import Avatar from "../ui/Avatar.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import Icon from "../ui/Icon.svelte";
  import Overlay from "../ui/Overlay.svelte";

  const STAGGER_MS = 40;
  const MAX_STAGGER_MS = 1200;

  let confirm = $state(null);

  const paneStyle = $derived(
    prefs.listPaneWidth > 0
      ? `width:${prefs.listPaneWidth}px;min-width:${prefs.listPaneWidth}px;flex:0 0 ${prefs.listPaneWidth}px`
      : null,
  );
  const selectedKeys = $derived([...subs.selected]);
  const allSelected = $derived(
    subs.visibleActive.length > 0 && subs.visibleActive.every((item) => subs.selected.has(item.sender_key)),
  );

  async function run(keys) {
    confirm = null;
    const { ok, failed } = await subs.unsubscribe(keys);
    mail.dropCachedViews();
    mail.refreshCounts();
    if (keys.length === 1 && ok === 1) showToast(t("toast.unsubscribed"));
    else if (keys.length === 1) showToast(t("toast.unsubscribe_failed"), "error", 4000);
    else if (failed) showToast(t("subs.bulk_result", { ok, failed }), "error", 4000);
    else showToast(t("subs.bulk_ok", { ok }));
  }

  async function openMessage(message) {
    try {
      mail.selectEmail(await getEmail(message.id));
    } catch (error) {
      showToast(String(error), "error");
    }
  }

  const countLabel = (item) => (item.count === 1 ? t("subs.email_one") : t("subs.emails", { n: item.count }));
</script>

{#snippet card(item, done, index)}
  {@const state = subs.status[item.sender_key]}
  {@const open = subs.expanded.has(item.sender_key)}
  {@const everything = subs.allMessages[item.sender_key]}
  {@const messages = Array.isArray(everything) ? everything : item.messages}
  <div
    class={["sub-card", done && "done", state === "error" && "failed", subs.selected.has(item.sender_key) && "selected"]}
    style:animation-delay={mail.animate ? `${Math.min(index * STAGGER_MS, MAX_STAGGER_MS)}ms` : null}
  >
    <div class="sub-main">
      {#if !done}
        <button
          class="sub-check"
          role="checkbox"
          aria-checked={subs.selected.has(item.sender_key)}
          aria-label={t("subs.select_all")}
          disabled={state === "busy"}
          onclick={() => subs.toggle(item.sender_key)}
        >
          <Icon name="check" size={12} />
        </button>
      {/if}
      <button class="sub-info" onclick={() => subs.toggleExpanded(item.sender_key)} aria-expanded={open} title={t("subs.show_messages")}>
        <Avatar sender={item.sender} mailbox="INBOX" />
        <span class="sub-text">
          <span class="sub-name">{sanitizeUnicodeNoise(item.name)}</span>
          <span class="sub-meta">
            {item.address} · {countLabel(item)}{#if item.unread_count} · {t("subs.unread", { n: item.unread_count })}{/if}
          </span>
        </span>
        <span class={["sub-chevron", open && "open"]}><Icon name="chevron-right" size={14} /></span>
      </button>
      {#if done}
        <span class="sub-done"><Icon name="check" size={12} />{t("subs.done")}</span>
      {:else}
        <button
          class={["unsubscribe-btn", "sub-unsub", state === "error" && "retry"]}
          disabled={state === "busy"}
          onclick={() => run([item.sender_key])}
        >
          {state === "busy" ? t("subs.unsubscribing") : state === "error" ? t("subs.retry") : t("subs.unsubscribe")}
        </button>
      {/if}
    </div>
    {#if open}
      <div class="sub-messages">
        {#each messages as message (message.id)}
          <button class={["sub-message", !message.is_read && "unread", mail.selectedEmail?.id === message.id && "active"]} onclick={() => openMessage(message)}>
            <span class="sub-message-subject">{sanitizeUnicodeNoise(message.subject) || t("app.no_subject")}</span>
            <span class="sub-message-date">{formatListDate(message.date)}</span>
          </button>
        {/each}
        {#if item.count > messages.length}
          <button class="sub-show-all" disabled={everything === "loading"} onclick={() => subs.showAll(item.sender_key)}>
            {t("subs.show_all", { n: item.count })}
          </button>
        {/if}
      </div>
    {/if}
  </div>
{/snippet}

<div class="email-list-pane subs-pane" style={paneStyle}>
  <div class="list-header">
    <div class="list-title-row">
      <span class="list-title">{t("sidebar.subscriptions")}</span>
      <span class="list-count">{t("subs.count", { n: subs.active.length })}</span>
    </div>
    <div class="search-row">
      <div class="search-bar">
        <Icon name="search" />
        <input
          type="text"
          placeholder={t("subs.search")}
          value={subs.query}
          oninput={(event) => (subs.query = event.currentTarget.value)}
        />
      </div>
    </div>
    <div class="subs-toolbar">
      {#if selectedKeys.length}
        <span class="subs-selected">{t("subs.selected", { n: selectedKeys.length })}</span>
        <button class="chip" onclick={() => subs.clearSelection()}>{t("subs.clear")}</button>
        <button class="verdant-btn primary subs-bulk" disabled={subs.busy} onclick={() => (confirm = selectedKeys)}>
          {t("subs.unsubscribe_selected", { n: selectedKeys.length })}
        </button>
      {:else}
        <button class="chip" disabled={!subs.visibleActive.length} onclick={() => (allSelected ? subs.clearSelection() : subs.selectAllVisible())}>
          {t(allSelected ? "subs.clear" : "subs.select_all")}
        </button>
      {/if}
    </div>
  </div>

  <div class={["email-list", "subs-list", !mail.animate && "suppress-anim"]}>
    {#if subs.loading}
      <div class="list-loading"><div class="list-spinner"></div></div>
    {:else if subs.error}
      <div class="subs-empty">{subs.error}</div>
    {:else if !subs.items.length}
      <EmptyState icon="subscriptions" title={t("empty.subs.title")} body={t("subs.empty")} />
    {:else}
      {#each subs.visibleActive as item, index (item.sender_key)}
        {@render card(item, false, index)}
      {:else}
        {#if !subs.query.trim()}<EmptyState icon="check" title={t("empty.subs_done.title")} body={t("subs.empty_done")} />{/if}
      {/each}
      {#if subs.visibleDone.length}
        <div class="subs-section">{t("subs.done")}</div>
        {#each subs.visibleDone as item, index (item.sender_key)}
          {@render card(item, true, subs.visibleActive.length + index)}
        {/each}
      {/if}
    {/if}
  </div>
</div>

{#if confirm}
  <Overlay onclose={() => (confirm = null)} panelClass="unsubscribe-panel" label={t("subs.bulk_title", { n: confirm.length })}>
    <div class="verdant-head">
      <h2>{t("subs.bulk_title", { n: confirm.length })}</h2>
      <button class="verdant-close" aria-label={t("reading.close")} onclick={() => (confirm = null)}>×</button>
    </div>
    <p>{t("subs.bulk_body")}</p>
    <div class="verdant-actions">
      <button class="verdant-btn" onclick={() => (confirm = null)}>{t("reading.unsubscribe_cancel")}</button>
      <button class="verdant-btn primary" onclick={() => run(confirm)}>{t("subs.unsubscribe_selected", { n: confirm.length })}</button>
    </div>
  </Overlay>
{/if}
