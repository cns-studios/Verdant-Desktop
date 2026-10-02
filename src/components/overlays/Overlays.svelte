<script>
  import { fly } from "svelte/transition";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { portal } from "../../lib/portal.js";
  import { session } from "../../state/session.svelte.js";
  import { toasts, ui } from "../../state/ui.svelte.js";
  import { updates } from "../../state/updates.svelte.js";
  import SendingPopup from "../compose/SendingPopup.svelte";
  import Onboarding from "../onboarding/Onboarding.svelte";
  import SettingsModal from "../settings/SettingsModal.svelte";
  import AccountPopover from "../sidebar/AccountPopover.svelte";
  import SmartInboxSetup from "../smart/SmartInboxSetup.svelte";
  import Icon from "../ui/Icon.svelte";
  import CategoryPopup from "./CategoryPopup.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import UnsubscribeDialog from "./UnsubscribeDialog.svelte";
  import UpdateToast from "./UpdateToast.svelte";
  import WhatsNew from "./WhatsNew.svelte";

  function accountAdded(account) {
    ui.addAccountOpen = false;
    session.accountAdded(account);
  }
</script>

<div class="overlay-root" {@attach portal}>
  {#if ui.settingsOpen}
    <SettingsModal />
  {/if}
  {#if ui.accountPopoverOpen}
    <AccountPopover />
  {/if}
  {#if ui.addAccountOpen}
    <Onboarding onfinish={accountAdded} oncancel={() => (ui.addAccountOpen = false)} />
  {/if}
  {#if ui.smartSetup}
    <SmartInboxSetup resort={ui.smartSetup.resort} />
  {/if}
  {#if ui.unsubscribe}
    <UnsubscribeDialog email={ui.unsubscribe} />
  {/if}
  {#if ui.whatsNew}
    <WhatsNew version={ui.whatsNew.version} content={ui.whatsNew.content} />
  {/if}
  {#if updates.offer}
    <UpdateToast />
  {/if}
  {#if ui.contextMenu}
    {#key ui.contextMenu}
      <ContextMenu x={ui.contextMenu.x} y={ui.contextMenu.y} row={ui.contextMenu.row} />
    {/key}
  {/if}
  {#if ui.categoryPopup}
    <CategoryPopup x={ui.categoryPopup.x} y={ui.categoryPopup.y} onSelect={ui.categoryPopup.onSelect} />
  {/if}

  <SendingPopup />

  {#if ui.attachmentDownload}
    <div class="attachment-download-modal open">
      <div class="attachment-download-card" role="status" transition:fly={{ x: 80, duration: 240 }}>
        <div class={["attachment-download-icon", ui.attachmentDownload.done ? "is-success" : "is-spinning"]}>
          {#if ui.attachmentDownload.done}
            <Icon name="check" />
          {/if}
        </div>
        <div class="attachment-download-text">
          {t(ui.attachmentDownload.done ? "app.attachment_downloaded" : "app.attachment_downloading", {
            name: ui.attachmentDownload.name,
          })}
        </div>
      </div>
    </div>
  {/if}

  <div class="toast-wrap">
    {#each toasts as toast (toast.id)}
      <div class={["toast", toast.type]}>{toast.message}</div>
    {/each}
  </div>
</div>
