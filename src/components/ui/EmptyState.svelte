<script>
  import Icon from "./Icon.svelte";

  let { icon = "inbox", title, body = "", action = "", onaction = undefined } = $props();
</script>

<div class="empty-state">
  <span class="empty-state-icon"><Icon name={icon} size={20} /></span>
  <div class="empty-state-title">{title}</div>
  {#if body}<p class="empty-state-body">{body}</p>{/if}
  {#if action && onaction}
    <button class="verdant-btn" onclick={onaction}>{action}</button>
  {/if}
</div>
