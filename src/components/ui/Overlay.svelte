<script>
  import { fade, fly } from "svelte/transition";

  let { onclose, panelClass = "", label = undefined, children } = $props();
</script>

<div
  class="verdant-overlay open"
  role="presentation"
  transition:fade={{ duration: 100 }}
  onclick={(event) => event.target === event.currentTarget && onclose?.()}
>
  <div
    class={["verdant-panel", panelClass]}
    role="dialog"
    aria-modal="true"
    aria-label={label}
    in:fly={{ y: 6, duration: 120 }}
  >
    {@render children()}
  </div>
</div>
