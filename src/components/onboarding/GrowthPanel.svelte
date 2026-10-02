<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import { STAGES } from "./flow.svelte.js";

  const LEAF_PATH = "M0 0 C 10 -13, 32 -15, 46 -2 C 33 11, 12 12, 0 0 Z";
  const LEAVES = [
    { x: 60, y: 318, angle: 200 },
    { x: 57, y: 236, angle: -20 },
    { x: 61, y: 150, angle: 205 },
    { x: 60, y: 62, angle: -35 },
  ];

  let { stage } = $props();
</script>

<aside class="ob-panel">
  <div class="ob-wordmark">Verdant</div>
  <div class="ob-growth">
    <svg class="ob-stem" viewBox="30 0 60 400" preserveAspectRatio="xMidYMid meet" aria-hidden="true">
      <path
        class="ob-stem-line"
        pathLength="1"
        d="M60 396 C 60 350, 54 320, 58 280 S 64 200, 58 160 S 56 90, 60 50"
      />
      {#each LEAVES as leaf, index (index)}
        <g transform="translate({leaf.x} {leaf.y}) rotate({leaf.angle})">
          <path class={["ob-leaf", index <= stage && "grown"]} d={LEAF_PATH} />
        </g>
      {/each}
    </svg>
    <ol class="ob-stages">
      {#each STAGES as name, index (name)}
        <li
          data-stage={index}
          class={[index < stage && "done", index === stage && "current"]}
          aria-current={index === stage ? "step" : undefined}
        >
          {t(`ob.stage.${name}`)}
        </li>
      {/each}
    </ol>
  </div>
</aside>
