<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import { formatCode } from "../../lib/verificationCode.js";
  import CopyCodeButton from "../list/CopyCodeButton.svelte";
  import Icon from "../ui/Icon.svelte";

  let { code } = $props();
  let copied = $state(false);
</script>

<div class={["vc-card", copied && "copied"]}>
  <span class="vc-badge"><Icon name="shield-check" /></span>
  <div class="vc-main">
    <span class="vc-label">{t("code.label")}</span>
    <span class="vc-digits" role="img" aria-label={formatCode(code)}>
      <span aria-hidden="true">
        {#each [...code] as digit, i (i)}<span class="vc-digit" style:--i={i}>{digit}</span>{/each}<span class="vc-caret"></span>
      </span>
    </span>
  </div>
  <CopyCodeButton class="vc-copy" {code} bind:copied />
</div>
