import { Store } from "@tauri-apps/plugin-store";
import { sanitizeUnicodeNoise } from "./format.js";

const STORE_FILE = "verdant.contacts.json";
const STORE_KEY = "contacts";
const MAX_CONTACTS = 1200;

let _store = null;

async function getStore() {
  if (!_store) {
    _store = await Store.load(STORE_FILE);
  }
  return _store;
}


export let contactsByEmail = new Map();
let _loaded = false;

export async function ensureContactsLoaded() {
  if (_loaded) return;
  _loaded = true;
  try {
    const store = await getStore();
    const raw = await store.get(STORE_KEY);
    if (!Array.isArray(raw)) return;
    for (const item of raw) {
      const email = normalizeEmailAddress(item?.email || "");
      if (!email) continue;
      contactsByEmail.set(email, {
        email,
        name: sanitizeUnicodeNoise(item?.name || ""),
        updatedAt: Number(item?.updatedAt || 0) || Date.now(),
      });
      if (contactsByEmail.size >= MAX_CONTACTS) break;
    }
  } catch (err) {
    console.warn("contacts: failed to load from store", err);
  }
}

async function persistContacts() {
  try {
    const store = await getStore();
    const list = Array.from(contactsByEmail.values())
      .sort((a, b) => (b.updatedAt || 0) - (a.updatedAt || 0))
      .slice(0, MAX_CONTACTS);
    await store.set(STORE_KEY, list);
    await store.save();
  } catch (err) {
    console.warn("contacts: failed to persist", err);
  }
}

export function normalizeEmailAddress(input) {
  const value = sanitizeUnicodeNoise(input || "").toLowerCase();
  const match = value.match(/[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}/i);
  return match ? match[0].toLowerCase() : "";
}

export function parseContactToken(rawToken) {
  const clean = sanitizeUnicodeNoise(rawToken || "");
  if (!clean) return null;

  const email = normalizeEmailAddress(clean);
  if (!email) return null;

  const bracketName = clean.replace(/<[^>]+>/g, "").replace(/[\"']/g, "").trim();
  const bareName = clean.replace(email, "").replace(/[<>\"']/g, "").trim();
  const name = sanitizeUnicodeNoise(bracketName || bareName || "");
  return { email, name };
}

export function parseContactsFromHeader(headerValue) {
  const value = String(headerValue || "");
  if (!value.trim()) return [];

  return value
    .split(/[,;\n]+/)
    .map((token) => parseContactToken(token))
    .filter(Boolean);
}

export function upsertContact(rawEmail, rawName = "") {
  const email = normalizeEmailAddress(rawEmail);
  if (!email) return;

  const existing = contactsByEmail.get(email);
  const incomingName = sanitizeUnicodeNoise(rawName || "");
  const next = {
    email,
    name: incomingName || existing?.name || "",
    updatedAt: Date.now(),
  };

  contactsByEmail.set(email, next);

  if (contactsByEmail.size > MAX_CONTACTS) {
    const overflow = Array.from(contactsByEmail.values())
      .sort((a, b) => (a.updatedAt || 0) - (b.updatedAt || 0));
    const removeCount = contactsByEmail.size - MAX_CONTACTS;
    overflow.slice(0, removeCount).forEach((item) => contactsByEmail.delete(item.email));
  }

  persistContacts().catch((e) => console.warn("contacts: persist failed", e));
}

export function extractContactsFromEmailRecord(email) {
  const contacts = [];
  contacts.push(...parseContactsFromHeader(email?.sender || ""));
  contacts.push(...parseContactsFromHeader(email?.to_recipients || ""));
  contacts.push(...parseContactsFromHeader(email?.cc_recipients || ""));
  return contacts;
}

export function ingestContactsFromEmails(emails) {
  for (const email of emails || []) {
    const contacts = extractContactsFromEmailRecord(email);
    contacts.forEach((contact) => upsertContact(contact.email, contact.name));
  }
}
