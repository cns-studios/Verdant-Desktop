<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import { prefs, setListPaneWidth } from "../../state/prefs.svelte.js";

  const MIN_WIDTH = 260;
  const NARROW_WINDOW = 980;
  const maxWidth = () => Math.max(MIN_WIDTH, Math.min(window.innerWidth * 0.68, 760));
  const clamp = (width) => Math.max(MIN_WIDTH, Math.min(Math.round(width), maxWidth()));

  let dragging = $state(false);

  $effect(() => {
    document.body.classList.toggle("resizing", dragging);
  });

  function startResize(event) {
    if (window.innerWidth <= NARROW_WINDOW) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    const startX = event.clientX;
    const startWidth = event.currentTarget.previousElementSibling.getBoundingClientRect().width;
    dragging = true;

    const move = (moveEvent) => setListPaneWidth(clamp(startWidth + moveEvent.clientX - startX));
    const stop = () => {
      dragging = false;
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  }
</script>

<svelte:window onresize={() => prefs.listPaneWidth > maxWidth() && setListPaneWidth(clamp(prefs.listPaneWidth))} />

<div
  class="pane-resizer"
  role="separator"
  aria-orientation="vertical"
  aria-label={t("list.resize_label")}
  onpointerdown={startResize}
></div>
