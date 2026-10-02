<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import { categoryLabel, smart } from "../../state/smartInbox.svelte.js";
  import { ui } from "../../state/ui.svelte.js";
  import FloatingMenu from "./FloatingMenu.svelte";

  let { x, y, onSelect } = $props();

  function choose(category) {
    const select = onSelect;
    ui.categoryPopup = null;
    select(category);
  }
</script>

<FloatingMenu {x} {y} class="category-popup" onclose={() => (ui.categoryPopup = null)}>
  <div class="category-popup-title">{t("smart.choose_category")}</div>
  {#each smart.categories as category (category.slug)}
    <button class="category-popup-item" role="menuitem" onclick={() => choose(category)}>
      <span class="category-popup-dot" style:--category-color={category.color || "#6c7065"}></span>
      {categoryLabel(category)}
    </button>
  {/each}
</FloatingMenu>
