<script>
  import { fade, fly } from "svelte/transition";

  let { onclose, panelClass = "", label = undefined, children } = $props();
</script>

<div
  class="verdant-overlay open"
  role="presentation"
  transition:fade={{ duration: 180 }}
  onclick={(event) => event.target === event.currentTarget && onclose?.()}
>
  <div
    class={["verdant-panel", panelClass]}
    role="dialog"
    aria-modal="true"
    aria-label={label}
    in:fly={{ y: 12, duration: 180 }}
  >
    {@render children()}
  </div>
</div>
