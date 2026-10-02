import { getLocale, t } from "./i18n/index.svelte.js";

const UNICODE_NOISE = /[­͏؜᠎​-‏‪-‮⁠-⁩﻿]/g;

export function escapeHtml(input) {
  if (!input) return "";
  return String(input)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

export function sanitizeUnicodeNoise(input) {
  if (!input) return "";
  return input.replace(UNICODE_NOISE, "").replace(/[ \t]{2,}/g, " ").trim();
}

function parseMailDate(raw) {
  const cleaned = String(raw || "")
    .replace(/\s(?:GMT|UTC)?[+-]\d{4}\b/gi, "")
    .replace(/\s+\((?:GMT|UTC)[^)]*\)/gi, "")
    .trim();
  const date = new Date(cleaned);
  return { cleaned, date: Number.isNaN(date.getTime()) ? null : date };
}

export function formatListDate(raw) {
  const { cleaned, date } = parseMailDate(raw);
  if (!date) return cleaned;

  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const mailDay = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const daysAgo = Math.round((today - mailDay) / 86400000);

  if (daysAgo === 0) return date.toLocaleTimeString(getLocale(), { hour: "2-digit", minute: "2-digit" });
  if (daysAgo === 1) return t("app.yesterday");
  return date.toLocaleDateString(getLocale());
}

export function formatReadingDate(raw) {
  const { cleaned, date } = parseMailDate(raw);
  if (!date) return cleaned;
  return date.toLocaleString(getLocale(), {
    weekday: "short",
    month: "short",
    day: "numeric",
    year: new Date().getFullYear() === date.getFullYear() ? undefined : "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function formatAttachmentSize(size) {
  const bytes = Number(size || 0);
  if (!Number.isFinite(bytes) || bytes <= 0) return t("app.unknown_size");
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

const MAILBOX_TITLE_KEYS = {
  INBOX: "sidebar.inbox",
  STARRED: "sidebar.starred",
  ARCHIVE: "sidebar.archive",
  SENT: "sidebar.sent",
  DRAFT: "sidebar.drafts",
  TRASH: "sidebar.trash",
};

export function mailboxTitle(mailbox) {
  return t(MAILBOX_TITLE_KEYS[mailbox] ?? "sidebar.inbox");
}

export function senderName(sender) {
  return sanitizeUnicodeNoise(sender || "").replace(/<[^>]+>/g, "").replace(/['"]/g, "").trim();
}

export function formatParticipants(rawSenders, maxDisplay = 3) {
  if (!rawSenders) return t("app.unknown_sender");
  const names = [...new Set(
    rawSenders.split(",").map((s) => senderName(s) || sanitizeUnicodeNoise(s.trim())).filter(Boolean),
  )];
  if (names.length <= maxDisplay) return names.join(", ");
  return `${names.slice(0, maxDisplay).join(", ")} +${names.length - maxDisplay}`;
}

export function splitRecipients(header) {
  return sanitizeUnicodeNoise(header || "").split(",").map((v) => v.trim()).filter(Boolean);
}

export function htmlPreview(html, limit = 180) {
  return sanitizeUnicodeNoise(html || "")
    .replace(/<style[\s\S]*?<\/style>/gi, " ")
    .replace(/<script[\s\S]*?<\/script>/gi, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&nbsp;/gi, " ")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, limit);
}
