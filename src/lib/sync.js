import { syncMailboxPage, getEmails } from "../api.js";
import { ingestContactsFromEmails } from "./contacts.js";
import { t } from "./i18n.js";

const RESYNC_COOLDOWN_MS = 10 * 1000; 

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


export async function notifyNewEmails(_nextInbox) {
  
  
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
      if (onSynced) onSynced(mailbox, latest);
      return;
    }
  } catch (err) {
    console.warn("IMAP manual sync warning:", err);
  }

  
  if (mailbox !== "STARRED" && mailbox !== "ARCHIVE") {
    const next = await syncMailboxPage(mailbox, null);
    mailboxNextPageToken.set(mailbox, next || null);
  }

  const latest = await getEmails(mailbox);
  ingestContactsFromEmails(latest);

  if (onSynced) {
    onSynced(mailbox, latest);
  }
}


export function startPeriodicSync(_onSynced) {
  
}

export function stopPeriodicSync() {
  
}
