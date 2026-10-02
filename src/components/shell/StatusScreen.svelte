<script>
  let { title, detail, action = undefined, onaction = undefined } = $props();
</script>

<div class="status-screen">
  <div class="status-title">{title}</div>
  <div class="status-detail">{detail}</div>
  {#if action}
    <button class="status-action" onclick={onaction}>{action}</button>
  {/if}
</div>

<style>
  .status-screen {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    height: 100vh;
    color: var(--text-mid);
  }

  .status-title {
    font: 500 15px "Fraunces", serif;
    color: var(--text);
  }

  .status-detail {
    font-size: 13px;
  }

  .status-action {
    margin-top: 8px;
    padding: 8px 16px;
    border: none;
    border-radius: 8px;
    background: var(--green);
    color: #fff;
    font-family: inherit;
    cursor: pointer;
  }
</style>
