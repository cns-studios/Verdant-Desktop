<script>
  import { copyText } from "../../lib/clipboard.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { formatCode } from "../../lib/verificationCode.js";
  import { showToast } from "../../state/ui.svelte.js";
  import CopyCodeButton from "../list/CopyCodeButton.svelte";
  import Icon from "../ui/Icon.svelte";

  let { code } = $props();
  let copied = $state(false);
  let timer = null;

  async function copyFromDigits() {
    if (!(await copyText(code))) return showToast(t("code.copy_failed"), "error");
    copied = true;
    clearTimeout(timer);
    timer = setTimeout(() => (copied = false), 1800);
  }
</script>

<div class={["vc-card", copied && "copied"]}>
  <span class="vc-badge"><Icon name="key-round" /></span>
  <div class="vc-main">
    <button type="button" class="vc-digits" aria-label={formatCode(code)} title={t("code.copy")} onclick={copyFromDigits}>
      <span aria-hidden="true">
        {#each [...formatCode(code)] as char, i (i)}<span class="vc-d" style:--i={i}>{char === " " ? "\u00a0" : char}</span>{/each}
      </span>
    </button>
  </div>
  <CopyCodeButton class="vc-copy" {code} bind:copied />
</div>
