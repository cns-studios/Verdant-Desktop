<script>
  import { copyText } from "../../lib/clipboard.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { showToast } from "../../state/ui.svelte.js";
  import Icon from "../ui/Icon.svelte";

  const COPIED_MS = 1800;

  let { code, class: className, copied = $bindable(false) } = $props();
  let timer = null;

  async function copy(event) {
    event.stopPropagation();
    if (!(await copyText(code))) {
      showToast(t("code.copy_failed"), "error");
      return;
    }
    copied = true;
    clearTimeout(timer);
    timer = setTimeout(() => (copied = false), COPIED_MS);
  }
</script>

<button type="button" class={className} onclick={copy}>
  <span class="vc-copy-icon"><Icon name="copy" /><Icon name="check" /></span>
  <span class="vc-copy-text">{t(copied ? "code.copied" : "code.copy")}</span>
</button>
