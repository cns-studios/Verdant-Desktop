<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import { CATEGORY_PREFIX, mail } from "../../state/mail.svelte.js";
  import { categoryLabel, smart } from "../../state/smartInbox.svelte.js";
  import Icon from "../ui/Icon.svelte";

  let editing = $state(null);
  let draftName = $state("");

  function startRename(category) {
    editing = category.slug;
    draftName = categoryLabel(category);
  }

  async function saveRename() {
    const slug = editing;
    editing = null;
    await smart.rename(slug, draftName);
  }

  function open(category) {
    if (editing !== category.slug) mail.open(`${CATEGORY_PREFIX}${category.slug}`, { sync: false });
  }

  function focusAndSelect(input) {
    input.focus();
    input.select();
  }
</script>

<div class="smart-categories">
  {#each smart.categories as category (category.slug)}
    {@const isEditing = editing === category.slug}
    <div
      class={["nav-item", "smart-category", mail.categorySlug === category.slug && "active", isEditing && "editing"]}
      role="button"
      tabindex="0"
      onclick={() => open(category)}
      onkeydown={(event) => event.key === "Enter" && event.target === event.currentTarget && open(category)}
    >
      <span class="smart-category-dot" style:--category-color={category.color || "#6c7065"}></span>
      <span class="nav-text smart-category-name">
        {#if isEditing}
          <input
            class="smart-rename-input"
            maxlength="40"
            aria-label={t("smart.rename")}
            bind:value={draftName}
            {@attach focusAndSelect}
            onclick={(event) => event.stopPropagation()}
            onkeydown={(event) => {
              if (event.key === "Enter") saveRename();
              if (event.key === "Escape") {
                event.stopPropagation();
                editing = null;
              }
            }}
          />
        {:else}
          {categoryLabel(category)}
        {/if}
      </span>
      <button
        class="smart-rename"
        title={t(isEditing ? "smart.confirm_rename" : "smart.rename")}
        aria-label={t(isEditing ? "smart.confirm_rename" : "smart.rename")}
        onclick={(event) => {
          event.stopPropagation();
          if (isEditing) saveRename();
          else startRename(category);
        }}
      >
        <Icon name={isEditing ? "check" : "pencil"} />
      </button>
      {#if category.unread_count}
        <span class="nav-badge subtle">{category.unread_count}</span>
      {/if}
    </div>
  {/each}
</div>
