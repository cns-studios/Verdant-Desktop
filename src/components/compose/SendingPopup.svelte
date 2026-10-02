<script>
  import { fly } from "svelte/transition";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { compose } from "../../state/compose.svelte.js";
</script>

{#if compose.holding}
  <div class="sending-popup open" transition:fly={{ y: -40, duration: 250 }}>
    <div class="spinner"></div>
    <span class="text">{t("toast.sending_hold")}</span>
    <button class="cancel-btn" type="button" onclick={() => compose.cancelSend()}>{t("toast.cancel")}</button>
  </div>
{/if}
