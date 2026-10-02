<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import { mail } from "../../state/mail.svelte.js";
  import { ui } from "../../state/ui.svelte.js";
  import Overlay from "../ui/Overlay.svelte";

  let { email } = $props();
  let busy = $state(false);

  const close = () => (ui.unsubscribe = null);

  async function confirmUnsubscribe() {
    busy = true;
    await mail.unsubscribe(email);
    close();
  }
</script>

<Overlay onclose={close} panelClass="unsubscribe-panel" label={t("reading.unsubscribe_confirm_title")}>
  <div class="verdant-head">
    <h2>{t("reading.unsubscribe_confirm_title")}</h2>
    <button class="verdant-close" aria-label={t("reading.close")} onclick={close}>×</button>
  </div>
  <p>{t("reading.unsubscribe_confirm_body", { sender: email.sender || "" })}</p>
  <div class="verdant-actions">
    <button class="verdant-btn" onclick={close}>{t("reading.unsubscribe_cancel")}</button>
    <button class="verdant-btn primary" disabled={busy} onclick={confirmUnsubscribe}>
      {t(busy ? "reading.unsubscribing" : "reading.unsubscribe_confirm")}
    </button>
  </div>
</Overlay>
