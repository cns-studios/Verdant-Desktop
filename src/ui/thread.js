import { getInboxThreads, getThreadMessages, markThreadRead, markEmailsRead, archiveEmail, trashEmail, toggleStarred, setEmailReadStatus, openExternalUrl } from "../api.js";
import { escapeHtml, sanitizeUnicodeNoise, formatListDate, formatReadingDate } from "../lib/format.js";
import { sanitizeEmailHtml } from "../lib/sanitize.js";
import { showToast } from "../lib/toast.js";
import { t } from "../lib/i18n.js";
import { applySenderAvatar, buildActionMenu, buildRecipientsDetailsHtml, setReadingPaneHidden } from "./reading.js";
import { downloadAttachment } from "../api.js";
import { openComposeForReply, openComposeForForward } from "./compose.js";
import { refreshCounts } from "./sidebar.js";
import { icon } from "./icons.js";
import { checkboxHtml, refresh as refreshMultiSelect } from "./multiselect.js";
import { findVerificationCode, mentionsCode } from "../lib/verificationCode.js";
import { buildCodeCard, buildCodeChip } from "./codeCard.js";


let currentThreads = [];
let selectedThreadId = null;
let selectedThreadMessages = [];
let expandedMessageIds = new Set();
let onRefreshCallback = null;
let onCountsRefreshCallback = null;
let renderGeneration = 0;

// Codes found per conversation, so re-rendering the list never re-reads mail.
// Keyed by thread and newest message; `null` means "looked, none".
const threadCodes = new Map();
const MAX_CODE_LOOKUPS = 30;

const threadCodeKey = (thread) => `${thread.thread_id}:${thread.latest_ts || ""}`;

function showThreadCode(row, code, animate) {
  if (!code || row.querySelector(".vc-chip")) return;
  row.querySelector(".email-item-inner")?.appendChild(buildCodeChip(code, animate));
}

/**
 * The list only has a short preview. When that mentions a code without
 * showing it, read the newest message of those few conversations.
 */
async function lookUpThreadCodes(pending, generation) {
  for (const { thread, row } of pending.slice(0, MAX_CODE_LOOKUPS)) {
    if (generation !== renderGeneration) return;
    const key = threadCodeKey(thread);
    if (!threadCodes.has(key)) {
      try {
        const messages = await getThreadMessages(thread.thread_id);
        const latest = messages[messages.length - 1];
        threadCodes.set(key, latest ? messageCode(latest) : null);
      } catch {
        continue;
      }
    }
    if (row.isConnected) showThreadCode(row, threadCodes.get(key), true);
  }
}

function messageCode(message) {
  return findVerificationCode({ subject: message.subject, snippet: message.snippet, body: message.body_html });
}

function yieldToBrowser() {
  return new Promise(resolve => requestAnimationFrame(() => resolve()));
}


function formatParticipants(rawSenders, maxDisplay = 3) {
  if (!rawSenders) return t("app.unknown_sender");

  const seen = new Set();
  const names = rawSenders
    .split(",")
    .map(s => {
      const clean = sanitizeUnicodeNoise(s.trim());
      const nameOnly = clean
        .replace(/<[^>]+>/g, "")
        .replace(/['"]/g, "")
        .trim();
      return nameOnly || clean;
    })
    .filter(name => {
      if (!name || seen.has(name)) return false;
      seen.add(name);
      return true;
    });

  if (names.length <= maxDisplay) return names.join(", ");
  return `${names.slice(0, maxDisplay).join(", ")} +${names.length - maxDisplay}`;
}


export async function renderThreadList(threads, activeFilter, searchQuery, animate = false) {
  currentThreads = threads || [];
  const list = document.getElementById("email-list");
  if (!list) return;
  const generation = ++renderGeneration;

  list.innerHTML = "";
  list.classList.toggle("suppress-anim", !animate);

  const visible = currentThreads.filter(thread => {
    if (activeFilter === "Important") {
      const labels = (thread.labels || "").split(",");
      const isPromo = labels.some(l =>
        ["SPAM", "TRASH", "CATEGORY_PROMOTIONS"].includes(l.trim())
      );
      if (isPromo) {
         return false; 
      }
    }
    if (activeFilter === "Unread" && thread.is_read) return false;
    if (activeFilter === "Attachments" && !thread.has_attachments) return false;
    if (searchQuery) {
      const hay = `${thread.subject} ${thread.participants} ${thread.snippet}`.toLowerCase();
      if (!hay.includes(searchQuery.toLowerCase())) return false;
    }
    return true;
  });

  const countEl = document.querySelector(".list-count");
  if (countEl) countEl.textContent = t("list.count", { n: visible.length });

  const codeLookups = [];
  for (let i = 0; i < visible.length; i++) {
    if (generation !== renderGeneration || !list.isConnected) return;
    const thread = visible[i];
    const row = document.createElement("div");
    const isActive = thread.thread_id === selectedThreadId;
    row.className = `email-item${thread.is_read ? "" : " unread"}${isActive ? " active" : ""}`;
    row.dataset.threadId = thread.thread_id;
    if (animate) row.style.animationDelay = `${Math.min(i * 40, 1200)}ms`;

    const participants = formatParticipants(thread.participants);
    const count = thread.message_count > 1
      ? `<span class="thread-count">${thread.message_count}</span>`
      : "";

    row.innerHTML = `
      ${checkboxHtml()}
      ${thread.is_read ? "" : '<div class="unread-dot"></div>'}
      ${thread.starred ? `<span class="star-badge">${icon("star-filled", 18)}</span>` : ""}
      <div class="email-item-main">
        <div class="sender-avatar"></div>
        <div class="email-item-inner">
          <div class="email-top">
            <span class="email-sender">${escapeHtml(participants)}${count}</span>
            <span class="email-time">${escapeHtml(formatListDate(thread.latest_date))}</span>
          </div>
          <div class="email-subject">${escapeHtml(sanitizeUnicodeNoise(thread.subject || t("app.no_subject")))}</div>
          <div class="email-preview">${escapeHtml(sanitizeUnicodeNoise(thread.snippet || ""))}</div>
        </div>
      </div>
    `;

    const firstSender = (thread.participants || "").split(",")[0] || "";
    applySenderAvatar(row.querySelector(".sender-avatar"), firstSender, "INBOX");
    const codeKey = threadCodeKey(thread);
    if (!threadCodes.has(codeKey)) {
      const code = findVerificationCode(thread);
      if (code) threadCodes.set(codeKey, code);
      else if (mentionsCode(thread)) codeLookups.push({ thread, row });
      else threadCodes.set(codeKey, null);
    }
    showThreadCode(row, threadCodes.get(codeKey), animate);
    row.addEventListener("click", () => selectThread(thread, row));
    list.appendChild(row);
    if ((i + 1) % 24 === 0) await yieldToBrowser();
  }

  if (generation !== renderGeneration) return;
  refreshMultiSelect(list);
  if (codeLookups.length) lookUpThreadCodes(codeLookups, generation);
}


async function selectThread(thread, row) {
  selectedThreadId = thread.thread_id;
  selectedThreadMessages = [];

  document.querySelectorAll(".email-item").forEach(el => el.classList.remove("active"));
  row.classList.add("active");
  row.classList.remove("unread");
  row.querySelector(".unread-dot")?.remove();
  setReadingPaneHidden(false);

  const readingBody = document.querySelector(".reading-body");
  if (readingBody) {
    readingBody.innerHTML = `<div class="thread-loading">${escapeHtml(t("toast.fetching"))}</div>`;
  }

  try {
    const messages = await getThreadMessages(thread.thread_id);
    selectedThreadMessages = messages;
    renderThreadPane(thread, messages);

    // Opening a conversation reads all of it. Marking only the message that
    // happens to be expanded left the others unread, so the row turned unread
    // again on the next list refresh.
    const unread = messages.filter(m => !m.is_read);
    if (unread.length || !thread.is_read) {
      unread.forEach(m => { m.is_read = true; });
      thread.is_read = true;
      thread.unread_count = 0;
      markEmailsRead(unread.map(m => m.id))
        .catch(err => console.error("Failed to mark conversation as read", err))
        .finally(() => {
          markThreadRowRead(thread.thread_id);
          refreshCounts().catch(() => {});
        });
    }
  } catch (err) {
    if (readingBody) {
      readingBody.innerHTML = `<div class="thread-loading" style="color:#8a3b3b">${escapeHtml(String(err))}</div>`;
    }
  }
}


function markThreadRowRead(threadId) {
  document.querySelectorAll(".email-item[data-thread-id]").forEach(row => {
    if (row.dataset.threadId !== threadId) return;
    row.classList.remove("unread");
    row.querySelector(".unread-dot")?.remove();
  });
}

// Longer conversations open with only the newest message expanded.
const COLLAPSE_ABOVE = 2;

function renderThreadPane(thread, messages) {
  const subjectEl = document.querySelector(".reading-subject");
  if (subjectEl) subjectEl.textContent = sanitizeUnicodeNoise(thread.subject || t("app.no_subject"));

  const metaEl = document.querySelector(".reading-meta");
  if (metaEl) metaEl.style.display = "none";

  updateThreadActionStates(thread, messages);

  const readingBody = document.querySelector(".reading-body");
  if (!readingBody) return;

  expandedMessageIds = new Set(
    (messages.length > COLLAPSE_ABOVE ? messages.slice(-1) : messages).map((message) => message.id)
  );

  readingBody.innerHTML = "";

  if (messages.length > 1) {
    const participantBar = document.createElement("div");
    participantBar.className = "thread-participant-bar";
    participantBar.innerHTML = `
      <span class="thread-participant-label">${escapeHtml(formatParticipants(thread.participants, 8))}</span>
      <span class="thread-message-total">${messages.length} ${t("thread.messages")}</span>
    `;
    readingBody.appendChild(participantBar);
  }

  const stack = document.createElement("div");
  stack.className = "thread-stack";
  readingBody.appendChild(stack);

  for (const message of messages) {
    stack.appendChild(buildMessageBubble(message, messages));
  }

  // With many collapsed messages above it, the open one can start below the
  // fold; bring it into view.
  const open = stack.querySelector(".thread-bubble.expanded");
  if (open && messages.length > COLLAPSE_ABOVE
      && open.getBoundingClientRect().top > readingBody.getBoundingClientRect().bottom - 120) {
    open.scrollIntoView({ block: "start" });
  }
}


function buildMessageBubble(message, allMessages) {
  const bubble = document.createElement("div");
  bubble.dataset.messageId = message.id;
  fillBubble(bubble, message, allMessages);
  return bubble;
}

/** Renders a bubble in its current state; also used when it is toggled. */
function fillBubble(bubble, message, allMessages) {
  const isExpanded = expandedMessageIds.has(message.id);
  bubble.className = `thread-bubble${isExpanded ? " expanded" : " collapsed"}`;

  const senderName = sanitizeUnicodeNoise(message.sender || t("app.unknown_sender"))
    .replace(/<[^>]+>/g, "")
    .replace(/['"]/g, "")
    .trim();

  if (isExpanded) {
    bubble.innerHTML = buildExpandedBubble(message, senderName);
    mountMessageBody(bubble, message);
    const code = messageCode(message);
    if (code) bubble.querySelector(".thread-bubble-body")?.prepend(buildCodeCard(code));
    bindBubbleButtons(bubble, message, allMessages);
    bindRecipientsToggle(bubble, message);
  } else {
    bubble.innerHTML = buildCollapsedBubble(message, senderName);
  }

  const avatar = bubble.querySelector(".thread-bubble-avatar");
  if (avatar) applySenderAvatar(avatar, message.sender || "", "INBOX");

  const header = bubble.querySelector(".thread-bubble-header");
  if (header) {
    header.addEventListener("click", () => toggleBubble(bubble, message, allMessages));
    header.addEventListener("keydown", (e) => {
      if (e.target !== header || (e.key !== "Enter" && e.key !== " ")) return;
      e.preventDefault();
      toggleBubble(bubble, message, allMessages);
    });
  }
}

function mountMessageBody(bubble, message) {
  const host = bubble.querySelector("[data-email-shadow-host]");
  if (host) {
    const rawHtml = message.body_html || `<pre>${escapeHtml(message.snippet || "")}</pre>`;
    const sanitized = sanitizeEmailHtml(sanitizeUnicodeNoise(rawHtml));
    const shadow = host.attachShadow({ mode: "closed" });
    shadow.innerHTML = `
      <style>
        :host {
          display: block;
          overflow-x: auto;
          -webkit-user-select: text;
          user-select: text;
        }
        .email-center {
          max-width: 640px;
          margin: 0 auto;
        }
        p { margin-bottom: 12px; }
        p:last-child { margin-bottom: 0; }
        pre {
          white-space: pre-wrap;
          word-break: break-word;
          background: var(--surface, #f0f0ec);
          border: 1px solid var(--border, #d6d9d2);
          border-radius: 8px;
          padding: 10px 12px;
          font-size: 12px;
        }
        img { max-width: 100%; height: auto; }
        a { color: var(--green, #4a5e45); }
        table { max-width: 100%; overflow-x: auto; display: block; }
        * { box-sizing: border-box; }
      </style>
      <div class="email-center">${sanitized}</div>
    `;

    shadow.querySelectorAll("a[href]").forEach((a) => {
      const originalHref = a.getAttribute("href") || "";
      a.setAttribute("data-verdant-href", originalHref);
      a.setAttribute("href", "#");
      a.setAttribute("target", "_self");
      a.setAttribute("rel", "noopener noreferrer");
    });

    const handleLinkIntent = (e) => {
      const target = e.target instanceof Element ? e.target : e.target?.parentElement;
      const a = target?.closest?.("a[href]");
      if (!a) return;

      const href = a.getAttribute("data-verdant-href") || a.getAttribute("href");
      e.preventDefault();
      e.stopPropagation();

      if (!(href && (href.startsWith("http://") || href.startsWith("https://")))) {
        return;
      }

      openExternalUrl(href).catch((error) => {
        console.error("External link open failed", error);
      });
    };

    shadow.addEventListener("click", handleLinkIntent, true);
    shadow.addEventListener("auxclick", handleLinkIntent, true);
    shadow.addEventListener("keydown", (e) => {
      if (e.key !== "Enter") return;
      handleLinkIntent(e);
    }, true);
  }
}

function buildCollapsedBubble(message, senderName) {
  const preview = sanitizeUnicodeNoise(message.snippet || "")
    || sanitizeUnicodeNoise(message.body_html || "")
      .replace(/<style[\s\S]*?<\/style>/gi, " ")
      .replace(/<script[\s\S]*?<\/script>/gi, " ")
      .replace(/<[^>]+>/g, " ")
      .replace(/&nbsp;/gi, " ")
      .replace(/\s+/g, " ")
      .trim()
      .slice(0, 180);
  return `
    <div class="thread-bubble-header" role="button" tabindex="0" aria-expanded="false">
      <div class="thread-bubble-avatar"></div>
      <div class="thread-bubble-meta-collapsed">
        <span class="thread-bubble-sender">${escapeHtml(senderName)}</span>
        <span class="thread-bubble-preview">${escapeHtml(preview)}</span>
      </div>
      <span class="thread-bubble-date">${escapeHtml(formatListDate(message.date))}</span>
      ${message.has_attachments ? `<span class="thread-bubble-attach-icon" title="${escapeHtml(t("thread.has_attachment"))}">${icon("paperclip", 14)}</span>` : ""}
    </div>
  `;
}

function buildExpandedBubble(message, senderName) {
  const attachments = parseAttachments(message);
  const attachLabel = attachments.length === 1
    ? t("thread.attachments", { n: 1 })
    : t("thread.attachments_plural", { n: attachments.length });

  const attachmentsHtml = attachments.length ? `
    <div class="thread-bubble-attachments">
      <div class="thread-attachments-label">${escapeHtml(attachLabel)}</div>
      ${attachments.map((a, i) => `
        <div class="thread-attachment-item">
          <span class="thread-attachment-name" title="${escapeHtml(a.filename || "attachment")}">${escapeHtml(a.filename || "attachment")}</span>
          <button class="thread-attachment-dl" data-attachment-index="${i}">${t("thread.download")}</button>
        </div>
      `).join("")}
    </div>
  ` : "";

  return `
    <div class="thread-bubble-header" role="button" tabindex="0" aria-expanded="true">
      <div class="thread-bubble-avatar"></div>
      <div class="thread-bubble-meta-expanded">
        <span class="thread-bubble-sender">${escapeHtml(senderName)}</span>
        <span class="thread-bubble-to" title="${t("reading.expand_recipients")}" data-recipients-expanded="false">${t("reading.to_x", { name: escapeHtml(sanitizeUnicodeNoise(message.to_recipients || t("reading.to_me"))) })}</span>
      </div>
      <span class="thread-bubble-date">${escapeHtml(formatReadingDate(message.date))}</span>
    </div>
    <div class="thread-bubble-body">
      <div class="thread-bubble-content email-body-text" data-email-shadow-host></div>
      ${attachmentsHtml}
    </div>
    <div class="thread-bubble-actions">
      <button class="thread-reply-btn" data-action="reply">
        ${icon("arrow-back-up", 15)}
        ${t("thread.reply")}
      </button>
      <button class="thread-reply-btn" data-action="forward">
        ${icon("arrow-forward-up", 15)}
        ${t("thread.forward")}
      </button>
    </div>
  `;
}


function bindRecipientsToggle(bubble, message) {
  const toEl = bubble.querySelector(".thread-bubble-to");
  if (!toEl) return;
  const collapsed = t("reading.to_x", {
    name: sanitizeUnicodeNoise(message.to_recipients || t("reading.to_me")),
  });
  toEl.addEventListener("click", (e) => {
    e.stopPropagation();
    const isExpanded = toEl.dataset.recipientsExpanded === "true";
    if (isExpanded) {
      toEl.textContent = collapsed;
      toEl.dataset.recipientsExpanded = "false";
    } else {
      toEl.innerHTML = buildRecipientsDetailsHtml(message) || collapsed;
      toEl.dataset.recipientsExpanded = "true";
    }
  });
}

function toggleBubble(bubble, message, allMessages) {
  if (expandedMessageIds.has(message.id)) {
    if (expandedMessageIds.size <= 1) return;
    expandedMessageIds.delete(message.id);
  } else {
    expandedMessageIds.add(message.id);
  }
  fillBubble(bubble, message, allMessages);
}


function bindBubbleButtons(bubble, message, allMessages) {
  bubble.querySelector('[data-action="reply"]')?.addEventListener("click", (e) => {
    e.stopPropagation();
    openComposeForReply(message);
  });

  bubble.querySelector('[data-action="forward"]')?.addEventListener("click", (e) => {
    e.stopPropagation();
    openComposeForForward(message);
  });

  bubble.querySelectorAll(".thread-attachment-dl").forEach(btn => {
    btn.addEventListener("click", async (e) => {
      e.stopPropagation();
      const idx = Number(btn.getAttribute("data-attachment-index"));
      const attachments = parseAttachments(message);
      const attachment = attachments[idx];
      if (!attachment) return;

      btn.disabled = true;
      btn.textContent = t("thread.downloading");
      try {
        const response = await downloadAttachment(
          message.id,
          attachment.attachment_id,
          attachment.filename || "attachment",
          attachment.mime_type || "application/octet-stream"
        );
        showToast(t("app.attachment_downloaded", { name: response.filename || attachment.filename || "attachment" }));
      } catch (err) {
        console.error("Attachment download error:", err);
        showToast(t("toast.attachment_failed"), "error");
      } finally {
        btn.disabled = false;
        btn.textContent = t("thread.download");
      }
    });
  });
}


function parseAttachments(message) {
  if (!message?.attachments_json) return [];
  try {
    const parsed = JSON.parse(message.attachments_json);
    return Array.isArray(parsed) ? parsed.filter(a => a?.attachment_id) : [];
  } catch { return []; }
}


function updateThreadActionStates(thread, messages) {
  const buttons = Array.from(document.querySelectorAll(".reading-actions .icon-btn"));

  const unsubBtn = document.querySelector(".reading-actions .unsubscribe-btn");
  if (unsubBtn) {
    const latest = messages && messages.length > 0 ? messages[messages.length - 1] : null;
    const hasUnsub = latest?.list_unsubscribe && latest.list_unsubscribe.trim().length > 0;
    if (!hasUnsub) {
      unsubBtn.style.display = "none";
    } else {
      unsubBtn.style.display = "";
      if (latest?.unsubscribed) {
        unsubBtn.classList.add("unsubscribed");
        unsubBtn.textContent = t("reading.unsubscribed");
        unsubBtn.disabled = true;
      } else {
        unsubBtn.classList.remove("unsubscribed");
        unsubBtn.textContent = t("reading.unsubscribe");
        unsubBtn.disabled = false;
      }
    }
  }

  buttons.forEach(btn => {
    if (!btn.dataset.action) {
      const initialTitle = btn.getAttribute("title") || "";
      if (initialTitle === t("reading.archive") || initialTitle === t("reading.restore")) btn.dataset.action = "archive";
      else if (initialTitle === t("reading.delete") || initialTitle === t("reading.permanent_delete")) btn.dataset.action = "delete";
      else if (initialTitle === t("reading.mark_unread")) btn.dataset.action = "mark_unread";
      else if (initialTitle === t("reading.star")) btn.dataset.action = "star";
      else if (initialTitle === t("reading.more")) btn.dataset.action = "more";
      else if (initialTitle === t("reading.close")) btn.dataset.action = "close";
    }

    const action = btn.dataset.action;

    if (action === "archive") {
      btn.style.display = "";
      btn.setAttribute("title", t("reading.archive"));
      btn.innerHTML = icon("archive");
    }
    
    if (action === "delete") {
      btn.style.display = "";
      btn.classList.add("danger");
      btn.setAttribute("title", t("reading.delete"));
      btn.innerHTML = icon("trash");
    }

    if (action === "mark_unread") {
      btn.style.display = "";
    }

    if (action === "star") {
      btn.style.display = "";
      btn.classList.toggle("active", !!thread?.starred);
    }
  });
}



function resetReadingPane() {
  selectedThreadId = null;
  selectedThreadMessages = [];
  const body = document.querySelector(".reading-body");
  const subject = document.querySelector(".reading-subject");
  const meta = document.querySelector(".reading-meta");
  if (body) body.innerHTML = "";
  if (subject) subject.textContent = "";
  if (meta) meta.style.display = "";
}


export function getSelectedThreadId() {
  return selectedThreadId;
}

export function getThreadById(threadId) {
  return currentThreads.find((t) => t.thread_id === threadId) || null;
}

export function getSelectedThreadLatestMessage() {
  return selectedThreadMessages.length > 0 ? selectedThreadMessages[selectedThreadMessages.length - 1] : null;
}

export function clearSelectedThread() {
  selectedThreadId = null;
  selectedThreadMessages = [];
}
