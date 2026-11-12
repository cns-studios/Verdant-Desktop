<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import { mail } from "../../state/mail.svelte.js";
  import { ui } from "../../state/ui.svelte.js";
  import Icon from "../ui/Icon.svelte";
  import Overlay from "../ui/Overlay.svelte";

  let { email } = $props();

  let trashExisting = $state(false);
  let archiveExisting = $state(false);
  let markRead = $state(false);
  let blockSender = $state(false);
  let applyToFuture = $state(false);
  let busy = $state(false);

  const address = $derived((email.sender || "").match(/<([^>]+)>/)?.[1] ?? email.sender ?? "");
  // "Apply to future" repeats the chosen actions, so it needs at least one.
  const canApply = $derived(trashExisting || archiveExisting || markRead);
  const hasChoice = $derived(canApply || blockSender);

  const close = () => (ui.senderCleanup = null);

  // Trashing and archiving are alternatives for the same mail.
  const pickTrash = (on) => on && (archiveExisting = false);
  const pickArchive = (on) => on && (trashExisting = false);

  $effect(() => {
    if (!canApply) applyToFuture = false;
  });

  async function done() {
    busy = true;
    await mail.cleanUpSender(email, {
      trashExisting,
      archiveExisting,
      markRead,
      blockSender,
      applyToFuture: applyToFuture && canApply,
    });
    close();
  }
</script>

{#snippet option(label, checked, onchange, { disabled = false, hint = "" } = {})}
  <label class={["cleanup-option", disabled && "disabled"]}>
    <input type="checkbox" {checked} {disabled} onchange={(event) => onchange(event.currentTarget.checked)} />
    <span class="cleanup-box"><Icon name="check" size={13} /></span>
    <span class="cleanup-label">
      {label}
      {#if hint}<span class="cleanup-hint">{hint}</span>{/if}
    </span>
  </label>
{/snippet}

<Overlay onclose={close} panelClass="cleanup-panel" label={t("cleanup.title")}>
  <div class="verdant-head">
    <h2>{t("cleanup.title")}</h2>
  </div>
  <p class="cleanup-sender" title={address}>{address}</p>
  <p class="cleanup-intro">{t("cleanup.body")}</p>

  <div class="cleanup-group">
    {@render option(t("cleanup.trash_existing"), trashExisting, (on) => ((trashExisting = on), pickTrash(on)))}
    {@render option(t("cleanup.archive_existing"), archiveExisting, (on) => ((archiveExisting = on), pickArchive(on)))}
    {@render option(t("cleanup.mark_read"), markRead, (on) => (markRead = on))}
  </div>
  <div class="cleanup-group">
    {@render option(t("cleanup.block_sender"), blockSender, (on) => (blockSender = on), { hint: t("cleanup.block_hint") })}
    {@render option(t("cleanup.apply_future"), applyToFuture, (on) => (applyToFuture = on), {
      disabled: !canApply || blockSender,
    })}
  </div>

  <div class="cleanup-actions">
    <button class="verdant-btn primary" disabled={busy || !hasChoice} onclick={done}>
      {t(busy ? "cleanup.working" : "cleanup.done")}
    </button>
    <button class="verdant-btn" disabled={busy} onclick={close}>{t("reading.unsubscribe_cancel")}</button>
  </div>
</Overlay>
