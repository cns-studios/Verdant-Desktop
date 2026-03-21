import { sanitizeUnicodeNoise } from "./format.js";

const CONTACTS_STORAGE_KEY = "verdant.contacts";
const MAX_CONTACTS = 1200;

export let contactsByEmail = loadContacts();

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

function loadContacts() {
  try {
    const raw = localStorage.getItem(CONTACTS_STORAGE_KEY);
    if (!raw) return new Map();
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return new Map();

    const map = new Map();
    for (const item of parsed) {
      const email = normalizeEmailAddress(item?.email || "");
      if (!email) continue;
      map.set(email, {
        email,
        name: sanitizeUnicodeNoise(item?.name || ""),
        updatedAt: Number(item?.updatedAt || 0) || Date.now(),
      });
      if (map.size >= MAX_CONTACTS) break;
    }
    return map;
  } catch {
    return new Map();
  }
}

function persistContacts() {
  try {
    const list = Array.from(contactsByEmail.values())
      .sort((a, b) => (b.updatedAt || 0) - (a.updatedAt || 0))
      .slice(0, MAX_CONTACTS);
    localStorage.setItem(CONTACTS_STORAGE_KEY, JSON.stringify(list));
  } catch {
  }
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

  persistContacts();
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
