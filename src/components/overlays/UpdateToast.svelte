<script>
  import { fly } from "svelte/transition";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { updates } from "../../state/updates.svelte.js";

  const offer = $derived(updates.offer);
  const working = $derived(["downloading", "installing", "restarting"].includes(offer.phase));
</script>

<div class="update-toast open" transition:fly={{ y: 20, duration: 300 }}>
  <div class="update-toast-header">
    <div>
      <div class="update-toast-title">{t("update.title", { version: offer.info.latestVersion })}</div>
      <div class="update-toast-sub">{offer.info.releaseName || ""}</div>
    </div>
    {#if !working}
      <button class="update-toast-close" aria-label={t("reading.close")} onclick={() => updates.dismissOffer()}>×</button>
    {/if}
  </div>

  {#if offer.phase !== "offer"}
    <div class="update-progress-wrap visible">
      <div class="update-progress-label">
        {offer.phase === "failed" ? t("update.failed", { error: offer.error }) : t(`update.${offer.phase}`)}
      </div>
      <div class="update-progress-track">
        <div
          class={["update-progress-bar", working && "indeterminate"]}
          style:background={offer.phase === "failed" ? "#c08d8d" : null}
        ></div>
      </div>
    </div>
  {/if}

  {#if !working}
    <div class="update-toast-actions">
      <button class="update-toast-btn" onclick={() => updates.dismissOffer()}>
        {t(offer.phase === "failed" ? "reading.close" : "update.later")}
      </button>
      {#if offer.phase === "offer"}
        <button class="update-toast-btn primary" onclick={() => updates.installOffer()}>{t("update.download")}</button>
      {/if}
    </div>
  {/if}
</div>
