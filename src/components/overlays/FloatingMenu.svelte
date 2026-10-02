<script>
  const EDGE_PADDING = 8;

  let { x, y, class: className, onclose, children } = $props();

  let menu;
  let position = $state({ left: 0, top: 0 });

  $effect(() => {
    const rect = menu.getBoundingClientRect();
    position = {
      left: Math.max(EDGE_PADDING, Math.min(x, window.innerWidth - rect.width - EDGE_PADDING)),
      top: Math.max(EDGE_PADDING, Math.min(y, window.innerHeight - rect.height - EDGE_PADDING)),
    };
  });

  function closeIfOutside(event) {
    if (!menu.contains(event.target)) onclose();
  }
</script>

<svelte:window onblur={onclose} onscrollcapture={onclose} />
<svelte:document onclickcapture={closeIfOutside} oncontextmenucapture={closeIfOutside} />

<div class={className} role="menu" bind:this={menu} style:left="{position.left}px" style:top="{position.top}px">
  {@render children()}
</div>
