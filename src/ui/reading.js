import { setEmailReadStatus, toggleStarred, archiveEmail, trashEmail } from "../api.js";
import { escapeHtml, sanitizeUnicodeNoise, formatReadingDate, formatAttachmentSize } from "../lib/format.js";
import { showToast } from "../lib/toast.js";
import { downloadAttachment } from "../api.js";
import { openExternalUrl } from "../api.js";
import { t } from "../lib/i18n.js";

function senderInitials(sender) {
  const raw = sanitizeUnicodeNoise(sender || "?").replace(/<.*?>/g, "").trim();
  const parts = raw.split(/\s+/).filter(Boolean);
  if (parts.length) {
    return parts.slice(0, 2).map((w) => w[0] || "").join("").toUpperCase() || "?";
  }
  const addr = extractSenderAddress(raw);
  if (!addr) return "?";
  return (addr[0] || "?").toUpperCase();
}

function extractSenderAddress(sender) {
  const clean = sanitizeUnicodeNoise(sender || "");
  const bracketMatch = clean.match(/<([^>]+)>/);
  let email = (bracketMatch ? bracketMatch[1] : clean).trim().toLowerCase();
  if (!email.includes("@")) {
    const token = email.split(/[\s,;]+/).find((part) => part.includes("@"));
    email = token ? token.trim().toLowerCase() : "";
  }
  return email;
}

function senderAvatarUrls(sender, mailbox = "") {
  if ((mailbox || "").toUpperCase() === "SENT") return [];
  const email = extractSenderAddress(sender);
  if (!email || !email.includes("@")) return [];
  const domain = email.split("@")[1];
  if (!domain || domain === "localhost") return [];
  return [
    `https://logo.clearbit.com/${encodeURIComponent(domain)}`,
    `https://www.google.com/s2/favicons?domain=${encodeURIComponent(domain)}&sz=64`,
  ];
}

export function applySenderAvatar(container, sender, mailbox = "") {
  if (!container) return;
  container.classList.remove("has-image");
  container.innerHTML = "";
  container.textContent = senderInitials(sender);

  const urls = senderAvatarUrls(sender, mailbox);
  if (!urls.length) return;

  const img = document.createElement("img");
  img.alt = "Sender icon";
  let idx = 0;

  img.onload = () => {
    if (img.naturalWidth <= 16 && img.naturalHeight <= 16) {
      idx += 1;
      if (idx < urls.length) { img.src = urls[idx]; return; }
      return;
    }
    container.classList.add("has-image");
    container.textContent = "";
    container.innerHTML = "";
    container.appendChild(img);
  };

  img.onerror = () => {
    idx += 1;
    if (idx < urls.length) img.src = urls[idx];
  };

  img.src = urls[idx];
}

function renderRecipientsLine(email) {
  const metaTo = document.querySelector(".meta-to");
  if (!metaTo) return;

  const toList = sanitizeUnicodeNoise(email.to_recipients || "")
    .split(",").map((v) => v.trim()).filter(Boolean);
  const ccList = sanitizeUnicodeNoise(email.cc_recipients || "")
    .split(",").map((v) => v.trim()).filter(Boolean);

  const merged = [...toList, ...ccList];
  const mailbox = (email.mailbox || "").toUpperCase();
  let collapsed = mailbox === "SENT" ? t("reading.recipients_loading") : t("reading.to_me");
  if (merged.length === 1) collapsed = t("reading.to_x", { name: merged[0] });
  if (merged.length > 1) collapsed = t("reading.to_x_others", { name: merged[0], n: merged.length - 1 });

  const expanded = [
    toList.length ? `${t("compose.to")}: ${toList.join(", ")}` : "",
    ccList.length ? `${t("compose.cc")}: ${ccList.join(", ")}` : "",
  ].filter(Boolean).join(" | ");

  metaTo.textContent = collapsed;
  metaTo.dataset.isExpanded = "false";
  metaTo.style.cursor = "pointer";
  metaTo.title = t("reading.expand_recipients");

  metaTo.onclick = null;
  metaTo.onclick = () => {
    const isExpanded = metaTo.dataset.isExpanded === "true";
    if (isExpanded) {
      metaTo.textContent = collapsed;
      metaTo.dataset.isExpanded = "false";
    } else {
      metaTo.textContent = expanded || collapsed;
      metaTo.dataset.isExpanded = "true";
    }
  };
}

function parseEmailAttachments(email) {
  if (!email?.attachments_json) return [];
  try {
    const parsed = JSON.parse(email.attachments_json);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

export function hasEmailAttachments(email) {
  const raw = email?.has_attachments;
  if (raw === true || raw === 1 || raw === "1") return true;
  if (typeof raw === "string" && raw.toLowerCase() === "true") return true;
  return parseEmailAttachments(email).length > 0;
}

function base64ToBytes(base64) {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

function showAttachmentDownloadModal(filename) {
  document.getElementById("attachment-download-modal")?.remove();
  const modal = document.createElement("div");
  modal.id = "attachment-download-modal";
  modal.className = "attachment-download-modal";
  modal.innerHTML = `
    <div class="attachment-download-card" role="dialog" aria-live="polite">
      <div class="attachment-download-icon is-spinning"></div>
      <div class="attachment-download-text">${t("app.attachment_downloading", { name: escapeHtml(filename || "attachment") })}</div>
    </div>
  `;
  document.body.appendChild(modal);
  requestAnimationFrame(() => modal.classList.add("open"));
}

async function showAttachmentDownloadSuccess(filename) {
  const modal = document.getElementById("attachment-download-modal");
  if (!modal) return;
  const icon = modal.querySelector(".attachment-download-icon");
  const text = modal.querySelector(".attachment-download-text");
  if (icon) {
    icon.classList.remove("is-spinning");
    icon.classList.add("is-success");
    icon.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20 7L10 17l-5-5"/></svg>`;
  }
  if (text) text.textContent = t("app.attachment_downloaded", { name: filename || "attachment" });
  await new Promise((resolve) => setTimeout(resolve, 1500));
}

function hideAttachmentDownloadModal() {
  const modal = document.getElementById("attachment-download-modal");
  if (!modal) return;
  modal.classList.remove("open");
  setTimeout(() => modal.remove(), 240);
}

async function handleAttachmentDownload(emailId, attachment) {
  if (!emailId || !attachment?.attachment_id) {
    showToast(t("toast.attachment_unavailable"), "error", 2400);
    return;
  }
  showAttachmentDownloadModal(attachment.filename || "attachment");
  try {
    const response = await downloadAttachment(
      emailId,
      attachment.attachment_id,
      attachment.filename || "attachment",
      attachment.mime_type || "application/octet-stream"
    );
    const bytes = base64ToBytes(response.data_base64 || "");
    const blob = new Blob([bytes], { type: response.content_type || attachment.mime_type || "application/octet-stream" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = response.filename || attachment.filename || "attachment";
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    URL.revokeObjectURL(url);
    await showAttachmentDownloadSuccess(response.filename || attachment.filename || "attachment");
  } finally {
    hideAttachmentDownloadModal();
  }
}

function renderReadingAttachments(email) {
  const readingBody = document.querySelector(".reading-body");
  if (!readingBody) return;

  readingBody.querySelector(".email-attachments")?.remove();

  const attachments = parseEmailAttachments(email).filter((a) => a && a.attachment_id);
  if (!attachments.length) return;

  const section = document.createElement("section");
  section.className = "email-attachments";
  section.innerHTML = `
    <div class="email-attachments-title">${t("thread.attachments_plural", { n: attachments.length })}</div>
    <div class="email-attachment-list">
      ${attachments.map((a, index) => `
        <div class="email-attachment-item">
          <div class="email-attachment-meta">
            <div class="email-attachment-name" title="${escapeHtml(a.filename || "attachment")}">${escapeHtml(a.filename || "attachment")}</div>
            <div class="email-attachment-sub">${escapeHtml(a.mime_type || "file")} • ${escapeHtml(formatAttachmentSize(a.size))}</div>
          </div>
          <button class="email-attachment-download" data-attachment-index="${index}">${t("thread.download")}</button>
        </div>
      `).join("")}
    </div>
  `;

  const bodyText = readingBody.querySelector(".email-body-text");
  if (bodyText) readingBody.insertBefore(section, bodyText);
  else readingBody.appendChild(section);

  section.querySelectorAll(".email-attachment-download").forEach((button) => {
    button.addEventListener("click", async () => {
      const attachment = attachments[Number(button.getAttribute("data-attachment-index"))];
      if (!attachment) return;
      button.disabled = true;
      button.textContent = t("thread.downloading");
      try {
        await handleAttachmentDownload(email.id, attachment);
      } catch (error) {
        console.error("Attachment download failed", error);
        showToast(t("toast.attachment_failed"), "error", 2600);
      } finally {
        button.disabled = false;
        button.textContent = t("thread.download");
      }
    });
  });
}

function renderEmailContentSafely(container, htmlContent) {
  if (!container) return;
  
  container.innerHTML = "";
  
  const iframe = document.createElement("iframe");
  iframe.className = "email-sandbox";
  iframe.setAttribute("sandbox", "allow-same-origin");
  iframe.style.border = "none";
  iframe.style.width = "100%";
  iframe.style.height = "100%";
  
  container.appendChild(iframe);
  
  const iframeDoc = iframe.contentDocument || iframe.contentWindow.document;
  if (!iframeDoc) return;
  
  const safeHtml = `
    <!DOCTYPE html>
    <html>
    <head>
      <meta charset="UTF-8">
      <style>
        * { box-sizing: border-box; }
        body {
          font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
          margin: 0;
          padding: 16px;
          background: transparent;
          color: #1e2119;
          font-size: 14px;
          line-height: 1.5;
          word-wrap: break-word;
        }
        img {
          max-width: 100%;
          height: auto;
          display: block;
        }
        a {
          color: #4a5e45;
          text-decoration: underline;
        }
        script, style { display: none !important; }
        body, div, p, span, a, img {
          all: revert;
        }
      </style>
    </head>
    <body>
      ${htmlContent}
    </body>
    </html>
  `;
  
  iframeDoc.open();
  iframeDoc.write(safeHtml);
  iframeDoc.close();

  iframeDoc.querySelectorAll("a[href]").forEach((a) => {
    const originalHref = a.getAttribute("href") || "";
    a.setAttribute("data-verdant-href", originalHref);
    a.setAttribute("href", "#");
    a.setAttribute("target", "_self");
    a.setAttribute("rel", "noopener noreferrer");
  });

  const handleIframeLinkIntent = async (e) => {
    const rawTarget = e.target;
    const target = rawTarget instanceof Element ? rawTarget : rawTarget?.parentElement;
    const a = target?.closest?.("a[href]");
    if (!a) return;

    const href = a.getAttribute("data-verdant-href") || a.getAttribute("href");

    e.preventDefault();
    e.stopPropagation();

    if (!(href && (href.startsWith("http://") || href.startsWith("https://")))) {
      return;
    }

    try {
      await openExternalUrl(href);
    } catch (error) {
      console.error("External link open failed", error);
    }
  };

  iframeDoc.addEventListener("click", handleIframeLinkIntent, true);
  iframeDoc.addEventListener("auxclick", handleIframeLinkIntent, true);
  iframeDoc.addEventListener("keydown", (e) => {
    if (e.key !== "Enter") return;
    void handleIframeLinkIntent(e);
  }, true);
}

export function renderReadingPane(email, mailbox) {
  const subject = document.querySelector(".reading-subject");
  const from = document.querySelector(".meta-from");
  const date = document.querySelector(".meta-date");
  const avatar = document.querySelector(".meta-avatar");
  const metaEl = document.querySelector(".reading-meta");
  const readingBody = document.querySelector(".reading-body");

  if (readingBody) readingBody.innerHTML = "";

  let body = document.querySelector(".email-body-text");
  if (!body && readingBody) {
    body = document.createElement("div");
    body.className = "email-body-text";
    readingBody.appendChild(body);
  }

  if (metaEl) metaEl.style.display = "";

  if (subject) subject.textContent = sanitizeUnicodeNoise(email.subject || t("app.no_subject"));
  if (from) from.textContent = sanitizeUnicodeNoise(email.sender || t("app.unknown_sender"));
  if (date) date.textContent = formatReadingDate(email.date || "");
  if (body) {
    const htmlContent = sanitizeUnicodeNoise(email.body_html || "");
    if (!htmlContent) {
      body.innerHTML = `<pre>${escapeHtml(sanitizeUnicodeNoise(email.snippet || ""))}</pre>`;
    } else {
      renderEmailContentSafely(body, htmlContent);
    }
  }

  renderReadingAttachments(email);

  if (avatar) applySenderAvatar(avatar, email.sender || "", email.mailbox || "");

  renderRecipientsLine(email);
  updateTopActionStates(email, mailbox || email.mailbox);
}

export function updateTopActionStates(email, mailbox) {
  const buttons = Array.from(document.querySelectorAll(".reading-actions .icon-btn"));
  const currentMailbox = (mailbox || "INBOX").toUpperCase();
  const activeNav = document.querySelector(".sidebar .nav-item.active")?.dataset?.mailbox || "INBOX";
  const isDraft = email?.mailbox?.toUpperCase().includes("DRAFT") || currentMailbox.includes("DRAFT") || activeNav.toUpperCase().includes("DRAFT");
  const isTrash = email?.mailbox?.toUpperCase().includes("TRASH") || currentMailbox.includes("TRASH") || activeNav.toUpperCase().includes("TRASH");
  const isSent = email?.mailbox?.toUpperCase().includes("SENT") || currentMailbox.includes("SENT") || activeNav.toUpperCase().includes("SENT");

  buttons.forEach((btn) => {
    const action = btn.dataset.action;

    if (action === "star") {
      btn.style.display = isDraft || isSent || isTrash ? "none" : "";
      btn.classList.toggle("active", !!email?.starred);
    }

    if (action === "delete") {
      btn.style.display = "";
      btn.classList.add("danger");
      if (isTrash) {
        btn.setAttribute("title", t("reading.permanent_delete"));
        btn.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14H6L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/><path d="M9 6V4h6v2"/><line x1="9" y1="11" x2="15" y2="17"/><line x1="15" y1="11" x2="9" y2="17"/></svg>`;
      } else {
        btn.setAttribute("title", t("reading.delete"));
        btn.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14H6L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/><path d="M9 6V4h6v2"/></svg>`;
      }
    }

    if (action === "archive") {
      if (isTrash) {
        btn.style.display = "";
        btn.setAttribute("title", t("reading.restore"));
        btn.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7v6h6"/><path d="M21 17a9 9 0 00-9-9 9 9 0 00-6 2.3L3 13"/></svg>`;
      } else {
        btn.style.display = isDraft || isSent ? "none" : "";
        btn.setAttribute("title", t("reading.archive"));
        btn.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="21 8 21 21 3 21 3 8"/><rect x="1" y="3" width="22" height="5"/><line x1="10" y1="12" x2="14" y2="12"/></svg>`;
      }
    }

    if (action === "mark_unread") {
      btn.style.display = isSent ? "none" : "";
    }
  });
}

export function setReadingPaneHidden(hidden) {
  document.body.classList.toggle("reading-pane-hidden", !!hidden);
}

export function bindReadingActions(getSelected, setSelected, onRefresh, openCompose, getCurrentMailbox, getThreadId) {
  const buttons = Array.from(document.querySelectorAll(".reading-actions .icon-btn"));

  for (const button of buttons) {
    button.onclick = async () => {
      const email = getSelected();
      const currentBox = (getCurrentMailbox?.() || "INBOX").toUpperCase();
      const action = button.dataset.action || "";
      const title = button.getAttribute("title") || "";
      const threadId = getThreadId?.();
      const messageIds = threadId 
        ? Array.from(document.querySelectorAll(".thread-bubble")).map(b => b.dataset.messageId).filter(Boolean)
        : [];

      console.log(`[UI] Action clicked: ${action}. title: ${title}. threadId: ${threadId}. messageIds count: ${messageIds.length}. selectedEmailId: ${email?.id}`);

      if (action === "archive") {
        if (title === t("reading.restore")) {
          const { restoreFromTrash } = await import("../api.js");
          if (threadId && messageIds.length) {
            for (const id of messageIds) await restoreFromTrash(id).catch(() => {});
          } else if (email) {
            await restoreFromTrash(email.id);
          }
          showToast(t("toast.restored"));
        } else {
          if (threadId && messageIds.length) {
            for (const id of messageIds) await archiveEmail(id).catch(() => {});
          } else if (email) {
            await archiveEmail(email.id);
          }
          showToast(t("toast.archived"));
        }
        await onRefresh();
        return;
      }

      if (action === "delete") {
        const threadId = getThreadId?.();
        const messageIds = threadId 
          ? Array.from(document.querySelectorAll(".thread-bubble")).map(b => b.dataset.messageId).filter(Boolean)
          : [];

        if (title === t("reading.permanent_delete")) {
          const { permanentDeleteEmail } = await import("../api.js");
          if (threadId && messageIds.length) {
            for (const id of messageIds) await permanentDeleteEmail(id).catch(() => {});
          } else if (email) {
            await permanentDeleteEmail(email.id);
          }
          showToast(t("toast.permanently_deleted"));
        } else {
          if (threadId && messageIds.length) {
            for (const id of messageIds) await trashEmail(id).catch(() => {});
          } else if (email) {
            await trashEmail(email.id);
          } else {
            console.warn("Trash clicked but no message IDs found.");
          }
          showToast(t("toast.trashed"));
        }
        await onRefresh();
        return;
      }

      if (action === "mark_unread") {
        const threadId = getThreadId?.();
        if (threadId) {
          const messages = Array.from(document.querySelectorAll(".thread-bubble"))
            .map(b => b.dataset.messageId).filter(Boolean);
          for (const id of messages) {
            await setEmailReadStatus(id, false).catch(() => {});
          }
          showToast(t("toast.unread_marked"));
          await onRefresh();
          return;
        }
        if (email) {
          const nextRead = !email.is_read;
          await setEmailReadStatus(email.id, nextRead);
          email.is_read = nextRead;
          showToast(nextRead ? t("toast.read_marked") : t("toast.unread_marked"));
          await onRefresh();
        }
        return;
      }

      if (action === "star") {
        const threadId = getThreadId?.();
        const messageIds = threadId 
          ? Array.from(document.querySelectorAll(".thread-bubble")).map(b => b.dataset.messageId).filter(Boolean)
          : [];

        if (threadId && messageIds.length) {
          for (const id of messageIds) await toggleStarred(id).catch(() => {});
          button.classList.toggle("active");
          showToast(t("toast.star_updated"));
        } else if (email) {
          await toggleStarred(email.id);
          email.starred = !email.starred;
          button.classList.toggle("active", !!email.starred);
          showToast(t("toast.star_updated"));
        }
        await onRefresh();
        return;
      }

      if (action === "more") {
        const threadId = getThreadId?.();
        const messageIds = threadId 
          ? Array.from(document.querySelectorAll(".thread-bubble")).map(b => b.dataset.messageId).filter(Boolean)
          : [];

        const isDraft = email?.mailbox?.toUpperCase().includes("DRAFT") || currentBox.includes("DRAFT");
        const isTrash = email?.mailbox?.toUpperCase().includes("TRASH") || currentBox.includes("TRASH");

        const entries = [
          {
            label: t("reading.mark_read"),
            onClick: async () => {
              if (threadId && messageIds.length) {
                const { markThreadRead } = await import("../api.js");
                await markThreadRead(threadId);
              } else if (email) {
                await setEmailReadStatus(email.id, true);
              }
            },
          },
          {
            label: t("reading.mark_unread_action"),
            onClick: async () => {
              if (threadId && messageIds.length) {
                for (const id of messageIds) await setEmailReadStatus(id, false).catch(() => {});
              } else if (email) {
                await setEmailReadStatus(email.id, false);
              }
            },
          },
          {
            label: t("reading.toggle_star"),
            onClick: async () => {
              if (threadId && messageIds.length) {
                for (const id of messageIds) await toggleStarred(id).catch(() => {});
              } else if (email) {
                await toggleStarred(email.id);
              }
            },
          },
          ...(isDraft && email ? [
            {
              label: t("reading.edit_draft"),
              onClick: async () => openCompose(email),
            },
            {
              label: t("reading.send_draft"),
              onClick: async () => {
                if (!email.draft_id) { showToast(t("toast.draft_no_id"), "error"); return; }
                const { sendExistingDraft } = await import("../api.js");
                await sendExistingDraft(email.draft_id);
                showToast(t("toast.draft_sent"));
              },
            },
          ] : []),
          ...(isTrash && email ? [
            {
              label: t("reading.restore"),
              onClick: async () => {
                const { restoreFromTrash } = await import("../api.js");
                await restoreFromTrash(email.id);
                showToast(t("toast.restored"));
              },
            },
            {
              label: t("reading.permanent_delete"),
              onClick: async () => {
                const { permanentDeleteEmail } = await import("../api.js");
                await permanentDeleteEmail(email.id);
                showToast(t("toast.permanently_deleted"));
              },
            },
          ] : []),
        ];

        buildActionMenu(entries, button, onRefresh);
        return;
      }

      if (action === "close") {
        setSelected(null);
        document.querySelectorAll(".email-item").forEach((el) => el.classList.remove("active"));
        setReadingPaneHidden(true);
      }
    };
  }
}

export function buildActionMenu(entries, anchor, onRefresh) {
  document.getElementById("action-menu")?.remove();
  const menu = document.createElement("div");
  menu.id = "action-menu";
  menu.className = "action-menu";

  entries.forEach((entry) => {
    const b = document.createElement("button");
    b.textContent = entry.label;
    b.onclick = async (e) => {
      e.stopPropagation();
      menu.remove();
      await entry.onClick();
      await onRefresh();
    };
    menu.appendChild(b);
  });

  anchor.style.position = "relative";
  anchor.appendChild(menu);
  setTimeout(() => {
    document.addEventListener("click", () => menu.remove(), { once: true });
  }, 0);
}
