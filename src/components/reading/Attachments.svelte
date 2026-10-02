<script>
  import { downloadAttachment } from "../../lib/api.js";
  import { parseAttachments } from "../../lib/attachments.js";
  import { formatAttachmentSize } from "../../lib/format.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { showToast, ui } from "../../state/ui.svelte.js";

  const SUCCESS_VISIBLE_MS = 1500;

  let { email, compact = false } = $props();
  let downloading = $state(null);

  const attachments = $derived(parseAttachments(email));
  const names = $derived(
    compact
      ? { box: "thread-bubble-attachments", title: "thread-attachments-label", item: "thread-attachment-item", name: "thread-attachment-name", button: "thread-attachment-dl" }
      : { box: "email-attachments", title: "email-attachments-title", item: "email-attachment-item", name: "email-attachment-name", button: "email-attachment-download" },
  );

  async function download(event, attachment) {
    event.stopPropagation();
    const filename = attachment.filename || "attachment";
    downloading = attachment.attachment_id;
    ui.attachmentDownload = { name: filename, done: false };
    try {
      const saved = await downloadAttachment(
        email.id,
        attachment.attachment_id,
        filename,
        attachment.mime_type || "application/octet-stream",
      );
      ui.attachmentDownload = { name: saved.filename || filename, done: true };
      await new Promise((resolve) => setTimeout(resolve, SUCCESS_VISIBLE_MS));
      showToast(t("app.attachment_downloaded", { name: saved.filename || filename }));
    } catch (error) {
      console.error("Attachment download failed", error);
      if (String(error) !== "Save cancelled") showToast(t("toast.attachment_failed"), "error", 2600);
    } finally {
      ui.attachmentDownload = null;
      downloading = null;
    }
  }
</script>

{#if attachments.length}
  <section class={names.box}>
    <div class={names.title}>
      {t(attachments.length === 1 && compact ? "thread.attachments" : "thread.attachments_plural", { n: attachments.length })}
    </div>
    <div class={compact ? null : "email-attachment-list"}>
      {#each attachments as attachment (attachment.attachment_id)}
        <div class={names.item}>
          {#if compact}
            <span class={names.name} title={attachment.filename || "attachment"}>{attachment.filename || "attachment"}</span>
          {:else}
            <div class="email-attachment-meta">
              <div class={names.name} title={attachment.filename || "attachment"}>{attachment.filename || "attachment"}</div>
              <div class="email-attachment-sub">{attachment.mime_type || "file"} • {formatAttachmentSize(attachment.size)}</div>
            </div>
          {/if}
          <button
            class={names.button}
            disabled={downloading === attachment.attachment_id}
            onclick={(event) => download(event, attachment)}
          >
            {t(downloading === attachment.attachment_id ? "thread.downloading" : "thread.download")}
          </button>
        </div>
      {/each}
    </div>
  </section>
{/if}
