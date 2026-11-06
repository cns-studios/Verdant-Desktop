<script>
  import { fly } from "svelte/transition";
  import { DUR, easeOut } from "../../lib/motion.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { compose } from "../../state/compose.svelte.js";
  import Icon from "../ui/Icon.svelte";

  $effect(() => {
    document.body.classList.toggle("is-sending", compose.busy);
    return () => document.body.classList.remove("is-sending");
  });
</script>

{#if compose.busy}
  <div class="sending-popup" role="status" transition:fly={{ y: -20, duration: DUR.base, easing: easeOut }}>
    <span class="sending-icon"><Icon name="send" size={16} /></span>
    <span class="text">{t("toast.sending_hold")}</span>
    {#if compose.holding}
      <button class="cancel-btn" type="button" onclick={() => compose.cancelSend()}>{t("toast.cancel")}</button>
      <span class="sending-track"><i></i></span>
    {/if}
  </div>
{/if}
