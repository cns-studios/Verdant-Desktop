<script>
  import { formatCode } from "../../lib/verificationCode.js";
  import Icon from "../ui/Icon.svelte";
  import CopyCodeButton from "./CopyCodeButton.svelte";

  let { code, animate = true } = $props();
  let copied = $state(false);
</script>

<div class={["vc-chip", animate && "vc-enter", copied && "copied"]}>
  <span class="vc-chip-code" ><Icon name="key-round" />{formatCode(code)}</span>
  <CopyCodeButton class="vc-chip-copy" {code} bind:copied />
</div>
