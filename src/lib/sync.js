import { syncMailboxPage, getEmails, getActiveAccountInfo, syncImapMailboxPage } from "../api.js";
import { ingestContactsFromEmails } from "./contacts.js";
import { t } from "./i18n.js";

const RESYNC_COOLDOWN_MS = 10 * 1000; 
const SYNC_TIMEOUT_MS = 8 * 1000;
const IMAP_FAILURE_COOLDOWN_MS = 5 * 60 * 1000;

export const mailboxNextPageToken = new Map();
export const lastSynced = new Map();
const imapUnavailableUntil = new Map();
let activeSyncs = 0;

export function setSyncBarVisible(visible) {
  const bar = document.getElementById("list-sync-bar");
  if (!bar) return;
  bar.classList.toggle("visible", !!visible);
}

function withTimeout(promise, timeoutMs, message) {
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(message)), timeoutMs);
  });
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
}

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

  activeSyncs += 1;
  setSyncBarVisible(true);
  try {
    const info = await getActiveAccountInfo();

    if (info?.provider === "imap") {
      const unavailableUntil = imapUnavailableUntil.get(mailbox) || 0;
      if (Date.now() < unavailableUntil) return;

      const currentOffset = mailboxNextPageToken.get(mailbox) || 0;
      if (currentOffset !== -1) {
        try {
          const hasMore = await withTimeout(
            syncImapMailboxPage(mailbox, currentOffset),
            SYNC_TIMEOUT_MS,
            `IMAP sync timed out for ${mailbox}`,
          );
          mailboxNextPageToken.set(mailbox, hasMore ? currentOffset + 50 : 0);
        } catch (error) {
          // Keep the local cache usable when the IMAP server is offline.
          imapUnavailableUntil.set(mailbox, Date.now() + IMAP_FAILURE_COOLDOWN_MS);
        }
      }
      const latest = await getEmails(mailbox);
      ingestContactsFromEmails(latest);
      if (onSynced) onSynced(mailbox, latest);
      return;
    }

    
    if (mailbox !== "STARRED" && mailbox !== "ARCHIVE") {
      const next = await withTimeout(
        syncMailboxPage(mailbox, null),
        SYNC_TIMEOUT_MS,
        `Mailbox sync timed out for ${mailbox}`,
      );
      mailboxNextPageToken.set(mailbox, next || null);
    }

    const latest = await withTimeout(
      getEmails(mailbox),
      SYNC_TIMEOUT_MS,
      `Loading ${mailbox} timed out`,
    );
    ingestContactsFromEmails(latest);

    if (onSynced) {
      onSynced(mailbox, latest);
    }

  } finally {
    activeSyncs = Math.max(0, activeSyncs - 1);
    if (activeSyncs === 0) setSyncBarVisible(false);
  }
}


export function startPeriodicSync(_onSynced) {
  
}

export function stopPeriodicSync() {
  
}
