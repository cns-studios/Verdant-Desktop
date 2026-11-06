<script>
  let { title = "", children } = $props();
</script>

<section class="set-group">
  {#if title}
    <h4 class="set-group-title">{title}</h4>
  {/if}
  <div class="set-list">
    {@render children()}
  </div>
</section>
