<script>
  import { fade, fly } from "svelte/transition";
  import { DUR, ease, easeOut } from "../../lib/motion.js";

  let { onclose, panelClass = "", label = undefined, children } = $props();
</script>

<div
  class="verdant-overlay open"
  role="presentation"
  transition:fade={{ duration: DUR.base, easing: ease }}
  onclick={(event) => event.target === event.currentTarget && onclose?.()}
>
  <div
    class={["verdant-panel", panelClass]}
    role="dialog"
    aria-modal="true"
    aria-label={label}
    in:fly={{ y: 8, duration: DUR.base, easing: easeOut }}
  >
    {@render children()}
  </div>
</div>
