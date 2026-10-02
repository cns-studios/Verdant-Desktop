<script>
  import { icons } from "../../lib/icons.js";

  let { name, size = 0 } = $props();
</script>

{#if size}
  <svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    width={size}
    height={size}
  >
    {@html icons[name]}
  </svg>
{:else}
  {@html icons[name]}
{/if}
