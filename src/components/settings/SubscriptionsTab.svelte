<script>
  import { onMount } from "svelte";
  import * as api from "../../lib/api.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { session } from "../../state/session.svelte.js";
  import { subscriptions } from "../../state/subscriptions.svelte.js";
  import { showToast } from "../../state/ui.svelte.js";
  import Icon from "../ui/Icon.svelte";
  import SettingsItem from "./SettingsItem.svelte";
  import SettingsSection from "./SettingsSection.svelte";

  let senders = $state([]);
  let loaded = $state(false);

  async function refresh() {
    try {
      senders = await api.getBlockedSenders();
    } catch (error) {
      console.error("Failed to load unsubscribed senders", error);
    } finally {
      loaded = true;
    }
  }

  onMount(refresh);
  $effect(() => {
    session.activeAccount?.id;
    refresh();
  });

  async function stop(sender) {
    if (!confirm(t("settings.subs.stop_confirm", { sender: sender.name }))) return;
    try {
      await api.unblockSender(sender.sender_key);
      senders = senders.filter((item) => item.sender_key !== sender.sender_key);
      subscriptions.load?.();
      showToast(t("settings.subs.stopped", { sender: sender.name }));
    } catch (error) {
      console.error("Failed to stop auto-archiving", error);
      showToast(t("settings.subs.failed"), "error");
    }
  }

  const since = (seconds) => (seconds ? new Date(seconds * 1000).toLocaleDateString() : "");
</script>

<SettingsSection>
  <SettingsItem label={t("settings.subs.title")} help={t("settings.subs.description")} />
</SettingsSection>
{#if session.accounts.length > 1 && session.activeAccount}
  <p class="set-note">{t("settings.subs.for_account", { email: session.activeAccount.email })}</p>
{/if}

{#if senders.length}
  <SettingsSection>
    <ul class="sis-rows subs-rows">
      {#each senders as sender (sender.sender_key)}
        <li class="sis-row subs-row">
          <span class="sis-row-name" title={sender.sender_key}>
            {sender.name}{#if sender.name !== sender.sender_key}<span class="subs-addr"> {sender.sender_key}</span>{/if}
          </span>
          <span class="sis-row-meta">{since(sender.created_at) ? t("settings.subs.since", { date: since(sender.created_at) }) : ""}</span>
          <button
            class="sis-icon-btn"
            aria-label={t("settings.subs.stop")}
            title={t("settings.subs.stop")}
            onclick={() => stop(sender)}
          >
            <Icon name="x" />
          </button>
        </li>
      {/each}
    </ul>
  </SettingsSection>
{:else if loaded}
  <p class="set-note">{t("settings.subs.empty")}</p>
{/if}
