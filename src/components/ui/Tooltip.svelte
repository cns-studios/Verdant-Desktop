<script>
  // Replaces native `title` tooltips app-wide: the first hover moves `title` to
  // `data-tip` (which silences the webview's own tooltip) and we render our own.
  const DELAY = 450;
  const GAP = 6;
  const MARGIN = 6;

  let text = $state("");
  let shown = $state(false);
  let x = $state(0);
  let y = $state(0);
  let bubble;
  let anchor = null;
  let timer = null;

  function tipOf(target) {
    const el = target instanceof Element ? target.closest("[title], [data-tip]") : null;
    if (!el) return null;
    const title = el.getAttribute("title");
    if (title !== null) {
      el.removeAttribute("title");
      if (title.trim()) el.dataset.tip = title;
    }
    return el.dataset.tip ? el : null;
  }

  function hide() {
    clearTimeout(timer);
    timer = null;
    anchor = null;
    shown = false;
  }

  function place() {
    if (!anchor?.isConnected || !bubble) return hide();
    const a = anchor.getBoundingClientRect();
    const w = bubble.offsetWidth;
    const h = bubble.offsetHeight;
    let top = a.bottom + GAP;
    if (top + h > window.innerHeight - MARGIN) top = Math.max(MARGIN, a.top - GAP - h);
    x = Math.min(Math.max(MARGIN, a.left + a.width / 2 - w / 2), window.innerWidth - w - MARGIN);
    y = top;
    shown = true;
  }

  function onOver(event) {
    const el = tipOf(event.target);
    if (el === anchor) return;
    hide();
    if (!el) return;
    anchor = el;
    text = el.dataset.tip;
    timer = setTimeout(() => requestAnimationFrame(place), DELAY);
  }

  function onOut(event) {
    if (anchor && !anchor.contains(event.relatedTarget)) hide();
  }
</script>

<svelte:document onmouseover={onOver} onmouseout={onOut} onmousedown={hide} onkeydown={hide} onscrollcapture={hide} />
<svelte:window onblur={hide} />

<div bind:this={bubble} class={["vt-tip", shown && "show"]} style:left="{x}px" style:top="{y}px" role="tooltip">{text}</div>
