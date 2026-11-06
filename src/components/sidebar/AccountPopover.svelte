<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import { detectProvider } from "../../lib/mailProviders.js";
  import { session } from "../../state/session.svelte.js";
  import { ui } from "../../state/ui.svelte.js";
  import { updates } from "../../state/updates.svelte.js";
  import Icon from "../ui/Icon.svelte";

  const close = () => (ui.accountPopoverOpen = false);

  const ACTIONS = [
    { icon: "circle-plus", label: "sidebar.add_account", run: () => (ui.addAccountOpen = true) },
    { icon: "news", label: "whatsnew.title", run: () => updates.openWhatsNew(true) },
    { icon: "settings", label: "settings.title", run: () => (ui.settingsOpen = true) },
  ];

  $effect(() => {
    session.refreshAccounts();
  });

  function switchTo(account) {
    if (account.is_active) return;
    close();
    session.switchAccount(account.id);
  }

  function remove(event, account) {
    event.stopPropagation();
    close();
    session.removeAccount(account);
  }

  const activate = (handler) => (event) => event.key === "Enter" && handler(event);
</script>

<div class="account-popover-backdrop" role="presentation" onclick={close}></div>
<div class="account-popover" role="menu">
  {#if session.accounts.length}
    <div class="account-popover-head">{t("sidebar.accounts")}</div>
    <div class="account-list">
      {#each session.accounts as account (account.id)}
        <div
          class={["account-item", account.is_active && "is-active"]}
          role="menuitemradio"
          aria-checked={account.is_active}
          tabindex="0"
          onclick={() => switchTo(account)}
          onkeydown={activate(() => switchTo(account))}
        >
          <div class={["account-avatar", account.provider === "imap" && "imap"]}>
            {(account.display_name || account.email).slice(0, 2).toUpperCase()}
          </div>
          <div class="account-item-info">
            <div class="account-item-name">{account.display_name || account.email}</div>
            <div class="account-item-provider" title={account.email}>
              {account.display_name ? account.email : account.provider === "imap" ? detectProvider(account.email).name : "Gmail"}
            </div>
          </div>
          {#if account.is_active}
            <span class="account-active-mark"><Icon name="check" size={16} /></span>
          {:else}
            <button class="account-remove-btn" title={t("reading.delete")} aria-label={t("reading.delete")} onclick={(event) => remove(event, account)}>
              <Icon name="x" size={14} />
            </button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <div class="account-popover-actions">
    {#each ACTIONS as action (action.label)}
      {@const run = () => {
        close();
        action.run();
      }}
      <div class="account-popover-action" role="menuitem" tabindex="0" onclick={run} onkeydown={activate(run)}>
        <Icon name={action.icon} />
        {t(action.label)}
      </div>
    {/each}
  </div>
</div>
