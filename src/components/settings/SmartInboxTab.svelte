<script>
  import { onMount } from "svelte";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { session } from "../../state/session.svelte.js";
  import { categoryLabel, smart } from "../../state/smartInbox.svelte.js";
  import { ui } from "../../state/ui.svelte.js";
  import Icon from "../ui/Icon.svelte";
  import SettingsItem from "./SettingsItem.svelte";
  import SettingsSection from "./SettingsSection.svelte";
  import Toggle from "./Toggle.svelte";

  let editing = $state(null);
  let draftName = $state("");

  onMount(() => smart.refresh());

  function openSetup(resort) {
    ui.settingsOpen = false;
    smart.setExpanded(true);
    ui.smartSetup = { resort };
  }

  async function toggle(on, input) {
    if (on) {
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

<SettingsSection>
  <SettingsItem label={t("settings.smart.title")} help={t("settings.smart.description")}>
    <Toggle label={t("settings.smart.title")} checked={smart.enabled} onchange={toggle} />
  </SettingsItem>
</SettingsSection>
{#if session.accounts.length > 1 && session.activeAccount}
  <p class="set-note">{t("settings.smart.for_account", { email: session.activeAccount.email })}</p>
{/if}

{#if smart.sorted}
  <SettingsSection title={t("settings.section.groups")}>
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
  </SettingsSection>
  <p class="set-note">{t("settings.smart.groups_help")}</p>

  <SettingsSection>
    <SettingsItem label={t("settings.smart.resort_title")} help={t("settings.smart.resort_body")}>
      <button class="verdant-btn" onclick={() => openSetup(true)}>{t("settings.smart.resort_button")}</button>
    </SettingsItem>
  </SettingsSection>
{:else if smart.enabled}
  <SettingsSection>
    <SettingsItem help={t("settings.smart.not_sorted")}>
      <button class="verdant-btn primary" onclick={() => openSetup(false)}>{t("smart.setup.sort")}</button>
    </SettingsItem>
  </SettingsSection>
{/if}
