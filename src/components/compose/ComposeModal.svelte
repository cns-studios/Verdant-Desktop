<script>
  import { tick } from "svelte";
  import { fileToAttachment } from "../../lib/attachments.js";
  import { eventCombo } from "../../lib/hotkeys.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { compose } from "../../state/compose.svelte.js";
  import { prefs } from "../../state/prefs.svelte.js";
  import Icon from "../ui/Icon.svelte";
  import FormatToolbar from "./FormatToolbar.svelte";
  import RecipientField from "./RecipientField.svelte";

  const MAX_ATTACHMENT_LABEL = 34;

  let editor = $state(null);
  let fileInput;
  let toField;

  $effect(() => {
    compose.editor = editor;
    return () => (compose.editor = null);
  });

  $effect(() => {
    const { target, tick: requested } = compose.focus;
    if (!requested || !compose.open) return;
    tick().then(() => {
      if (target === "to") return toField?.focus();
      editor?.focus();
      const firstBlock = editor?.querySelector("div");
      if (firstBlock) window.getSelection()?.setPosition(firstBlock, 0);
    });
  });

  const shorten = (name) =>
    name.length > MAX_ATTACHMENT_LABEL ? `${name.slice(0, MAX_ATTACHMENT_LABEL - 3)}...` : name;

  async function attachFiles() {
    const files = Array.from(fileInput.files || []);
    fileInput.value = "";
    compose.addAttachments(await Promise.all(files.map(fileToAttachment)));
  }

  function onKeydown(event) {
    if (eventCombo(event) !== prefs.hotkeys.send) return;
    event.preventDefault();
    event.stopPropagation();
    compose.send();
  }
</script>

<div
  class={["modal-overlay", compose.open && "open"]}
  role="presentation"
  onclick={(event) => event.target === event.currentTarget && compose.close()}
  onkeydown={onKeydown}
>
  <div class={["compose-modal", compose.maximized && "compose-maximized"]} role="dialog" aria-label={t("compose.title")}>
    <div class="modal-header">
      <span class="modal-title">{t("compose.title")}</span>
      <div class="modal-header-actions">
        <button
          class="modal-close"
          id="compose-max-btn"
          title={t("app.maximize")}
          aria-label={t("app.maximize")}
          onclick={() => (compose.maximized = !compose.maximized)}
        >
          <Icon name="square" />
        </button>
        <button class="modal-close" aria-label={t("reading.close")} onclick={() => compose.close()}>×</button>
      </div>
    </div>

    <div class="modal-fields">
      <RecipientField bind:this={toField} field="to" label={t("compose.to")} placeholder={t("compose.recipient_placeholder")} />
      <RecipientField field="cc" label={t("compose.cc")} placeholder={t("compose.cc_placeholder")} />
      <div class="modal-field">
        <label for="compose-subject">{t("compose.subject")}</label>
        <input id="compose-subject" type="text" placeholder={t("compose.subject_placeholder")} bind:value={compose.subject} />
      </div>
    </div>

    <div class="modal-body">
      <div
        class="compose-editor"
        contenteditable="true"
        role="textbox"
        aria-multiline="true"
        data-placeholder={t("compose.placeholder")}
        bind:this={editor}
        bind:innerHTML={compose.bodyHtml}
      ></div>
    </div>

    <FormatToolbar {editor} />

    <div class="compose-attachments">
      {#each compose.attachments as attachment, index (index)}
        <div class="compose-attachment">
          <span class="compose-attachment-name" title={attachment.filename}>
            {shorten((attachment.filename || t("compose.attachment_fallback")).trim())}
          </span>
          <button
            class="compose-attachment-remove"
            aria-label={t("compose.remove_attachment")}
            onclick={() => compose.removeAttachment(index)}
          >
            x
          </button>
        </div>
      {/each}
    </div>
    <input type="file" multiple hidden bind:this={fileInput} onchange={attachFiles} />

    <div class="modal-footer">
      <div class="modal-tools">
        <button class="modal-tool" title={t("compose.tool.attach")} onclick={() => fileInput.click()}>
          <Icon name="paperclip" />
        </button>
        <button
          class={["modal-tool", compose.formatting && "active"]}
          title={t("compose.tool.format")}
          onclick={() => compose.toggleFormatting()}
        >
          <Icon name="forms" />
        </button>
      </div>
      <div class="modal-footer-actions">
        <button
          class="modal-tool compose-clear-btn"
          title={t("compose.clear")}
          aria-label={t("compose.clear")}
          onclick={() => compose.clear()}
        >
          <Icon name="trash" />
        </button>
        <button class="verdant-btn" onclick={() => compose.saveDraft()}>{t("compose.save_draft")}</button>
        <button class="send-btn" disabled={compose.busy} onclick={() => compose.send()}>
          <Icon name="send" />
          {t("compose.send")}
        </button>
      </div>
    </div>
  </div>
</div>
