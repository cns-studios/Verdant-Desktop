import { syncMailboxPage, getEmails } from "../api.js";
import { ingestContactsFromEmails } from "./contacts.js";
import { showToast } from "./toast.js";
import { t } from "./i18n.js";

const SYNC_INTERVAL_MS = 45000;
const RESYNC_COOLDOWN_MS = 5 * 60 * 1000;

export const mailboxNextPageToken = new Map();
export const lastSynced = new Map();

const KNOWN_IDS_KEY = "verdant.knownInboxIds";
export let knownInboxIds = loadKnownIds();

function loadKnownIds() {
  try {
    const raw = localStorage.getItem(KNOWN_IDS_KEY);
    return raw ? new Set(JSON.parse(raw)) : new Set();
  } catch {
    return new Set();
  }
}

export function setKnownInboxIds(ids) {
  knownInboxIds = ids;
  localStorage.setItem(KNOWN_IDS_KEY, JSON.stringify(Array.from(ids)));
}

let syncTimer = null;

export async function notifyNewEmails(nextInbox) {
  const isFirstRun = localStorage.getItem(KNOWN_IDS_KEY) === null;
  const nextIds = new Set((nextInbox || []).map((m) => m.id));
  
  if (isFirstRun) {
    setKnownInboxIds(nextIds);
    return;
  }

  const unseen = (nextInbox || []).filter((m) => !knownInboxIds.has(m.id) && !m.is_read);
  setKnownInboxIds(nextIds);

  if (!unseen.length) return;
  
  const { appPrefs } = await import("../ui/settings.js");
  if (!appPrefs.showNotifications) return;

  const title = unseen.length === 1 
    ? t("toast.new_email")
    : t("toast.new_emails_plural", { n: unseen.length });
    
  const subject = (unseen[0].subject || t("app.no_subject")).replace(/[\u00AD\u034F\u061C\u180E\u200B-\u200F\u202A-\u202E\u2060-\u2069\uFEFF]/g, "");
  showToast(unseen.length === 1 ? `${title}: ${subject}` : title);

  if (!("Notification" in window)) return;
  
  try {
    const { isPermissionGranted, requestPermission, sendNotification } = await import("@tauri-apps/plugin-notification");
    let permissionGranted = await isPermissionGranted();
    if (!permissionGranted) {
      const permission = await requestPermission();
      permissionGranted = permission === "granted";
    }

    if (permissionGranted) {
      sendNotification({
        title: title,
        body: unseen.length === 1 
          ? `${unseen[0].sender} - ${unseen[0].subject}` 
          : t("toast.new_emails_plural", { n: unseen.length }),
        icon: "128x128.png",
      });
    }
  } catch (err) {
    console.error("Failed to send notification via plugin", err);
    // Fallback?
    if (Notification.permission === "granted") {
      new Notification(title, {
        body: unseen.length === 1 
          ? `${unseen[0].sender} - ${unseen[0].subject}` 
          : t("toast.new_emails_plural", { n: unseen.length }),
      });
    }
  }
}

export async function syncMailboxInBackground(mailbox, force = false, onSynced = null) {
  const key = mailbox;
  const now = Date.now();
  const last = lastSynced.get(key) || 0;

  if (!force && now - last < RESYNC_COOLDOWN_MS) return;
  lastSynced.set(key, now);

  try {
    const { getActiveAccountInfo, syncImapMailboxPage } = await import("../api.js");
    const info = await getActiveAccountInfo();
    if (info?.provider === "imap") {
      const currentOffset = mailboxNextPageToken.get(mailbox) || 0;
      if (currentOffset !== -1) {
        const hasMore = await syncImapMailboxPage(mailbox, currentOffset);
        if (hasMore) {
          mailboxNextPageToken.set(mailbox, currentOffset + 50);
        } else {
          mailboxNextPageToken.set(mailbox, -1);
        }
      }
      const latest = await getEmails(mailbox);
      ingestContactsFromEmails(latest);
      if (mailbox === "INBOX") await notifyNewEmails(latest);
      if (onSynced) onSynced(mailbox, latest);
      return;
    }
  } catch (err) {
    console.error("IMAP sync error:", err);
  }

  
  if (mailbox !== "STARRED" && mailbox !== "ARCHIVE") {
    showToast(t("toast.fetching"), "info", 1200);
    const next = await syncMailboxPage(mailbox, null);
    mailboxNextPageToken.set(mailbox, next || null);
  }

  const latest = await getEmails(mailbox);
  ingestContactsFromEmails(latest);

  if (mailbox === "INBOX") {
    await notifyNewEmails(latest);
  }

  if (onSynced) {
    onSynced(mailbox, latest);
  }
}

export function startPeriodicSync(onSynced) {
  if (syncTimer) clearInterval(syncTimer);
  syncTimer = setInterval(() => {
    syncMailboxInBackground("INBOX", false, onSynced).catch((e) =>
      console.error("Periodic sync failed", e)
    );
  }, SYNC_INTERVAL_MS);
}

export function stopPeriodicSync() {
  if (syncTimer) clearInterval(syncTimer);
  syncTimer = null;
}
