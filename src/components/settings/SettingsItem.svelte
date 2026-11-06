<script>
  let { label = "", help = "", active = false, error = false, title = undefined, disabled = false, children = undefined, below = undefined } = $props();
</script>

<div class={["set-item", disabled && "disabled"]} {title}>
  <div class="set-item-row">
    <div class="set-item-text">
      {#if label}<div class="set-item-label">{label}{#if active}<span class="set-active-dot"></span>{/if}</div>{/if}
      {#if help}<div class={["set-item-help", error && "error"]}>{help}</div>{/if}
    </div>
    {#if children}
      <div class="set-item-control">{@render children()}</div>
    {/if}
  </div>
  {#if below}
    <div class="set-item-below">{@render below()}</div>
  {/if}
</div>
