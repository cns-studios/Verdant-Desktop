<script>
  import { hexToOklch, oklchToHex, normalizeHex } from "../../lib/accent.js";

  let { value, onchange, label = "Accent color" } = $props();

  const SIZE = 148;
  const RING = 22;
  const MID = SIZE / 2 - RING / 2;

  const lch = $derived(hexToOklch(normalizeHex(value) ?? "#4a7fb5"));
  const angle = $derived(lch.h);
  let draft = $state(null);
  let dragging = false;
  let wheel;

  function apply(h, c = lch.c) {
    onchange(oklchToHex(0.55, Math.max(c, 0.04), h));
  }

  function hueFromEvent(event) {
    const rect = wheel.getBoundingClientRect();
    const x = event.clientX - (rect.left + rect.width / 2);
    const y = event.clientY - (rect.top + rect.height / 2);
    return ((Math.atan2(y, x) * 180) / Math.PI + 450) % 360; // 0° = top, clockwise
  }

  function down(event) {
    dragging = true;
    wheel.setPointerCapture(event.pointerId);
    apply(hueFromEvent(event));
  }
  function move(event) {
    if (dragging) apply(hueFromEvent(event));
  }
  function up() {
    dragging = false;
  }

  // The ring is painted in OKLCH hues, so ring angle == OKLCH hue and thumb/result stay in sync.
  const ringStops = Array.from({ length: 13 }, (_, i) => `${oklchToHex(0.7, 0.14, i * 30)} ${i * 30}deg`).join(", ");

  function key(event) {
    const step = event.shiftKey ? 15 : 3;
    if (event.key === "ArrowRight" || event.key === "ArrowUp") apply((angle + step) % 360);
    else if (event.key === "ArrowLeft" || event.key === "ArrowDown") apply((angle - step + 360) % 360);
    else return;
    event.preventDefault();
  }

  const thumbX = $derived(SIZE / 2 + MID * Math.sin((angle * Math.PI) / 180));
  const thumbY = $derived(SIZE / 2 - MID * Math.cos((angle * Math.PI) / 180));

  function commitHex(event) {
    const hex = normalizeHex(event.currentTarget.value);
    if (hex) onchange(hex);
    draft = null;
  }
</script>

<div class="cw">
  <div
    class="cw-wheel"
    bind:this={wheel}
    role="slider"
    tabindex="0"
    aria-label={label}
    aria-valuemin="0"
    aria-valuemax="360"
    aria-valuenow={Math.round(angle)}
    style:width="{SIZE}px"
    style:height="{SIZE}px"
    style:--ring-w="{RING}px"
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
    onkeydown={key}
  >
    <span class="cw-ring" style:background="conic-gradient(from 0deg, {ringStops})"></span>
    <span class="cw-thumb" style:left="{thumbX}px" style:top="{thumbY}px" style:background={value}></span>
  </div>

  <div class="cw-side">
    <label class="cw-field">
      <span>Hex</span>
      <input
        type="text"
        spellcheck="false"
        maxlength="7"
        value={draft ?? value}
        oninput={(e) => (draft = e.currentTarget.value)}
        onchange={commitHex}
        onblur={commitHex}
      />
    </label>
    <label class="cw-field">
      <span>Intensity</span>
      <input
        class="settings-range"
        type="range"
        min="0.02"
        max="0.2"
        step="0.002"
        value={Math.min(0.2, Math.max(0.02, lch.c))}
        oninput={(e) => apply(angle, Number(e.currentTarget.value))}
      />
    </label>
  </div>
</div>

<style>
  .cw { display: flex; gap: 18px; align-items: center; padding: 6px 0; }
  .cw-wheel {
    position: relative;
    flex-shrink: 0;
    touch-action: none;
    cursor: pointer;
    outline: none;
  }
  .cw-ring {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    -webkit-mask: radial-gradient(circle, transparent calc(50% - var(--ring-w)), #000 calc(50% - var(--ring-w) + 0.5px));
    mask: radial-gradient(circle, transparent calc(50% - var(--ring-w)), #000 calc(50% - var(--ring-w) + 0.5px));
  }
  .cw-wheel:focus-visible { outline: 2px solid var(--accent-light); outline-offset: 3px; }
  .cw-thumb {
    position: absolute;
    width: 18px;
    height: 18px;
    margin: -9px 0 0 -9px;
    border-radius: 50%;
    border: 3px solid #fff;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.3), 0 2px 6px rgba(0, 0, 0, 0.3);
    pointer-events: none;
  }
  .cw-side { display: grid; gap: 12px; flex: 1; min-width: 0; }
  .cw-field { display: grid; gap: 4px; font: 500 12px var(--font); color: var(--text-mid); }
  .cw-field input[type="text"] {
    font: 500 13px var(--font-mono, monospace);
    background: var(--white);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: 6px 8px;
    width: 100%;
    box-sizing: border-box;
  }
  .cw-field input[type="text"]:focus { border-color: var(--accent); outline: none; }
</style>
