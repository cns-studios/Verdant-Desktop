import { fetchRemoteImage } from "./api.js";
import { sanitizeUnicodeNoise } from "./format.js";

const cache = new Map();

function senderAddress(sender) {
  const clean = sanitizeUnicodeNoise(sender || "");
  const bracketed = clean.match(/<([^>]+)>/);
  const candidate = (bracketed ? bracketed[1] : clean).trim().toLowerCase();
  if (candidate.includes("@") && !/\s/.test(candidate)) return candidate;
  return candidate.split(/[\s,;]+/).find((part) => part.includes("@")) || "";
}

export function senderInitials(sender) {
  const words = sanitizeUnicodeNoise(sender || "?").replace(/<.*?>/g, "").trim().split(/\s+/).filter(Boolean);
  if (words.length) return words.slice(0, 2).map((w) => w[0] || "").join("").toUpperCase() || "?";
  return (senderAddress(sender)[0] || "?").toUpperCase();
}

export function avatarUrl(sender, mailbox = "") {
  if (mailbox.toUpperCase() === "SENT") return null;
  const domain = senderAddress(sender).split("@")[1];
  if (!domain || domain === "localhost") return null;
  return `https://www.google.com/s2/favicons?domain=${encodeURIComponent(domain)}&sz=64`;
}

export function cachedAvatar(url) {
  const hit = cache.get(url);
  return typeof hit === "string" ? hit : null;
}

export function loadAvatar(url) {
  if (!cache.has(url)) {
    const pending = fetchRemoteImage(url)
      .catch(() => null)
      .then((dataUrl) => {
        cache.set(url, dataUrl);
        return dataUrl;
      });
    cache.set(url, pending);
  }
  return Promise.resolve(cache.get(url));
}
