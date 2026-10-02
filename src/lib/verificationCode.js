const KEYWORDS = /\b(?:code|codes|otp|passcode|pin|one[- ]time|verification|verify|authenticat\w*|2fa|kode|tan|einmal\w*|best(?:ä|ae)tigungs\w*|sicherheits\w*|verifizierungs\w*|anmelde\w*|zugangs\w*|freischalt\w*)\b/gi;

const CANDIDATE = /\d(?:[ \u00a0]\d){5}|\d{3}[ \u00a0-]?\d{3}/g;

const NOT_A_LOGIN_CODE = /(?:zip|postal|post|area|country|promo|discount|coupon|voucher|order|tracking|qr|rabatt|gutschein|aktions|bestell|vorwahl)[- ]?$/i;

const MAX_TEXT = 20000;
const NEAR_BEFORE = 160;
const NEAR_AFTER = 80;

const ENTITIES = { nbsp: " ", amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" };

export function htmlToText(html) {
  return String(html || "")
    .slice(0, MAX_TEXT * 6)
    .replace(/<!--[\s\S]*?-->/g, " ")
    .replace(/<(style|script|title|head)\b[\s\S]*?<\/\1>/gi, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&#(\d+);/g, (_, n) => String.fromCharCode(Number(n)))
    .replace(/&([a-z]+);/gi, (m, name) => ENTITIES[name.toLowerCase()] ?? " ")
    .replace(/[\u00AD\u200B-\u200F\u2060\uFEFF]/g, "")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, MAX_TEXT);
}

function isPlausible(text, start, end) {
  const before = text.slice(Math.max(0, start - 2), start);
  const after = text.slice(end, end + 2);
  const prev = before.slice(-1);
  if (/[\d#$€£+=/\\@_&?]/.test(prev)) return false;
  if (/\d[.,:\-\u00a0 ]$/.test(before)) return false;
  if (/^(?:\d|[.,:\-/]\d|[%€$£@_=])/.test(after)) return false;
  if (/\D/.test(text.slice(start, end)) && /^[ \u00a0-]\d/.test(after)) return false;
  if (/^ ?(?:€|eur|usd|chf|kg|km|mb|gb)\b/i.test(text.slice(end, end + 5))) return false;
  return true;
}

function candidates(text) {
  const found = [];
  CANDIDATE.lastIndex = 0;
  let m;
  while ((m = CANDIDATE.exec(text))) {
    const start = m.index;
    const end = start + m[0].length;
    if (!isPlausible(text, start, end)) continue;
    const code = m[0].replace(/\D/g, "");
    if (/^(\d)\1{5}$/.test(code)) continue;
    found.push({ code, start, end });
  }
  return found;
}

function keywordPositions(text) {
  const positions = [];
  KEYWORDS.lastIndex = 0;
  let m;
  while ((m = KEYWORDS.exec(text))) {
    if (!NOT_A_LOGIN_CODE.test(text.slice(Math.max(0, m.index - 12), m.index))) positions.push(m.index);
  }
  return positions;
}

function pickNear(text) {
  const keys = keywordPositions(text);
  if (!keys.length) return null;
  let best = null;
  for (const c of candidates(text)) {
    for (const k of keys) {
      const distance = k <= c.start ? c.start - k : k - c.end;
      const limit = k <= c.start ? NEAR_BEFORE : NEAR_AFTER;
      if (distance > limit) continue;
      const score = k <= c.start ? distance : distance + NEAR_BEFORE;
      if (!best || score < best.score) best = { code: c.code, score };
    }
  }
  return best?.code || null;
}

export function findVerificationCode({ subject = "", snippet = "", body = "" } = {}) {
  const subjectText = htmlToText(subject);
  const bodyText = htmlToText(body) || htmlToText(snippet);

  const near = pickNear(subjectText) || pickNear(bodyText);
  if (near) return near;

  if (!keywordPositions(subjectText).length) return null;
  const distinct = [...new Set(candidates(bodyText).map(c => c.code))];
  return distinct.length === 1 ? distinct[0] : null;
}

export function mentionsCode({ subject = "", snippet = "" } = {}) {
  return keywordPositions(`${htmlToText(subject)} ${htmlToText(snippet)}`).length > 0;
}

export function formatCode(code) {
  return `${code.slice(0, 3)} ${code.slice(3)}`;
}
