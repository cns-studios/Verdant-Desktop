<script>
  import { onMount } from "svelte";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { session } from "../../state/session.svelte.js";
  import { categoryLabel, smart } from "../../state/smartInbox.svelte.js";
  import { ui } from "../../state/ui.svelte.js";
  import Icon from "../ui/Icon.svelte";

  let editing = $state(null);
  let draftName = $state("");

  onMount(() => smart.refresh());

  function openSetup(resort) {
    ui.settingsOpen = false;
    smart.setExpanded(true);
    ui.smartSetup = { resort };
  }

  async function toggle(event) {
    const input = event.currentTarget;
    if (input.checked) {
      input.checked = false;
      openSetup(false);
      return;
    }
    if (!confirm(t("settings.smart.confirm_off"))) {
      input.checked = true;
      return;
    }
    try {
      await smart.setEnabled(false);
    } catch (error) {
      console.error("Failed to turn off smart inbox", error);
      input.checked = true;
    }
  }

  function startRename(category) {
    editing = category.slug;
    draftName = categoryLabel(category);
  }

  async function finishRename(save) {
    const slug = editing;
    if (!slug) return;
    editing = null;
    if (save) await smart.rename(slug, draftName);
  }

  function focusAndSelect(input) {
    input.focus();
    input.select();
  }

  const meta = (category) =>
    category.unread_count
      ? t("settings.smart.group_meta_unread", { n: category.message_count, unread: category.unread_count })
      : t("settings.smart.group_meta", { n: category.message_count });
</script>

<div class="sis-settings-head">
  <div>
    <h3>{t("settings.smart.title")}</h3>
    <p>{t("settings.smart.description")}</p>
    {#if session.accounts.length > 1 && session.activeAccount}
      <p class="sis-for">{t("settings.smart.for_account", { email: session.activeAccount.email })}</p>
    {/if}
  </div>
  <label class="settings-switch sis-switch" title={t("settings.smart.title")}>
    <input type="checkbox" checked={smart.enabled} aria-label={t("settings.smart.title")} onchange={toggle} />
  </label>
</div>

{#if smart.sorted}
  <div class="settings-section-label">{t("settings.smart.groups")}</div>
  <ul class="sis-rows">
    {#each smart.categories as category (category.slug)}
      <li class={["sis-row", editing === category.slug && "editing"]} style:--group-color={category.color || "#7b8075"}>
        <span class="sis-dot"></span>
        <span class="sis-row-name" role="presentation" ondblclick={() => startRename(category)}>
          {#if editing === category.slug}
            <input
              class="sis-rename"
              maxlength="40"
              aria-label={t("smart.rename")}
              bind:value={draftName}
              {@attach focusAndSelect}
              onblur={() => finishRename(true)}
              onkeydown={(event) => {
                if (event.key === "Enter") finishRename(true);
                if (event.key === "Escape") {
                  event.stopPropagation();
                  finishRename(false);
                }
              }}
            />
          {:else}
            {categoryLabel(category)}
          {/if}
        </span>
        <span class="sis-row-meta">{meta(category)}</span>
        <button
          class="sis-icon-btn"
          aria-label={t("smart.rename")}
          title={t("smart.rename")}
          onclick={() => startRename(category)}
        >
          <Icon name="pencil" />
        </button>
      </li>
    {/each}
  </ul>
  <p class="settings-help">{t("settings.smart.groups_help")}</p>
  <div class="sis-resort">
    <div>
      <strong>{t("settings.smart.resort_title")}</strong>
      <p>{t("settings.smart.resort_body")}</p>
    </div>
    <button class="verdant-btn" onclick={() => openSetup(true)}>{t("settings.smart.resort_button")}</button>
  </div>
{:else if smart.enabled}
  <div class="sis-empty">
    <p>{t("settings.smart.not_sorted")}</p>
    <button class="verdant-btn primary" onclick={() => openSetup(false)}>{t("smart.setup.sort")}</button>
  </div>
{/if}
