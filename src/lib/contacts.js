import { Store } from "@tauri-apps/plugin-store";
import { sanitizeUnicodeNoise } from "./format.js";

const STORE_FILE = "verdant.contacts.json";
const STORE_KEY = "contacts";
const MAX_CONTACTS = 1200;
const PERSIST_DELAY_MS = 400;

const contacts = new Map();
let storePromise = null;
let loaded = false;
let persistTimer = null;

function store() {
  storePromise ??= Store.load(STORE_FILE);
  return storePromise;
}

export function normalizeEmailAddress(input) {
  const match = sanitizeUnicodeNoise(input || "").match(/[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}/i);
  return match ? match[0].toLowerCase() : "";
}

export function parseContactToken(rawToken) {
  const clean = sanitizeUnicodeNoise(rawToken || "");
  const email = normalizeEmailAddress(clean);
  if (!email) return null;
  const bracketName = clean.replace(/<[^>]+>/g, "").replace(/["']/g, "").trim();
  const bareName = clean.replace(email, "").replace(/[<>"']/g, "").trim();
  return { email, name: sanitizeUnicodeNoise(bracketName || bareName || "") };
}

export function parseContactsFromHeader(headerValue) {
  return String(headerValue || "").split(/[,;\n]+/).map(parseContactToken).filter(Boolean);
}

export async function loadContacts() {
  if (loaded) return;
  loaded = true;
  try {
    const raw = await (await store()).get(STORE_KEY);
    if (!Array.isArray(raw)) return;
    for (const item of raw.slice(0, MAX_CONTACTS)) {
      const email = normalizeEmailAddress(item?.email || "");
      if (!email) continue;
      contacts.set(email, {
        email,
        name: sanitizeUnicodeNoise(item?.name || ""),
        updatedAt: Number(item?.updatedAt || 0) || Date.now(),
      });
    }
  } catch (error) {
    console.warn("contacts: failed to load from store", error);
  }
}

function schedulePersist() {
  clearTimeout(persistTimer);
  persistTimer = setTimeout(async () => {
    try {
      const list = [...contacts.values()].sort((a, b) => b.updatedAt - a.updatedAt).slice(0, MAX_CONTACTS);
      const s = await store();
      await s.set(STORE_KEY, list);
      await s.save();
    } catch (error) {
      console.warn("contacts: failed to persist", error);
    }
  }, PERSIST_DELAY_MS);
}

export function contactName(email) {
  return contacts.get(email)?.name || "";
}

export function upsertContact(rawEmail, rawName = "") {
  const email = normalizeEmailAddress(rawEmail);
  if (!email) return;
  const name = sanitizeUnicodeNoise(rawName || "") || contacts.get(email)?.name || "";
  contacts.set(email, { email, name, updatedAt: Date.now() });

  if (contacts.size > MAX_CONTACTS) {
    [...contacts.values()]
      .sort((a, b) => a.updatedAt - b.updatedAt)
      .slice(0, contacts.size - MAX_CONTACTS)
      .forEach((item) => contacts.delete(item.email));
  }
  schedulePersist();
}

export function ingestContactsFromEmails(emails) {
  for (const email of emails || []) {
    for (const header of [email?.sender, email?.to_recipients, email?.cc_recipients]) {
      parseContactsFromHeader(header).forEach((contact) => upsertContact(contact.email, contact.name));
    }
  }
}

export function suggestContacts(query, excludedEmails, limit = 8) {
  const q = sanitizeUnicodeNoise(query || "").toLowerCase();
  if (!q) return [];
  return [...contacts.values()]
    .filter((c) => !excludedEmails.has(c.email) && `${c.name} ${c.email}`.toLowerCase().includes(q))
    .map((c) => ({ ...c, starts: c.email.startsWith(q) || c.name.toLowerCase().startsWith(q) }))
    .sort((a, b) => (a.starts === b.starts ? b.updatedAt - a.updatedAt : a.starts ? -1 : 1))
    .slice(0, limit);
}
