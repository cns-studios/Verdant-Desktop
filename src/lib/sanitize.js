import DOMPurify from "dompurify";

const IMAGE_HINT = /<(img|picture|source)\b|url\(|\sbackground\s*=/i;
const CSS_URL = /url\(/i;
// `html`/`body` used as type selectors (not `.body`, `#body`, `tbody`, ...).
const ROOT_SELECTOR = /(^|[\s,>+~(])(html|body)(?![\w-])/gi;
const HOST_SELECTOR = /:host|::slotted|::part/i;

// Forms could submit (and navigate the whole app) to arbitrary URLs.
const FORM_TAGS = ["form", "input", "button", "select", "textarea", "option", "optgroup", "datalist"];
const REMOTE_MEDIA_TAGS = ["img", "picture", "source", "image", "video", "audio", "track"];

export const MAIL_HTML_CLASS = "vd-mail-html";
export const MAIL_BODY_CLASS = "vd-mail-body";
export const MAIL_PLAIN_CLASS = "vd-mail-plain";

export const hasEmailImages = (html) => IMAGE_HINT.test(html || "");

function stripImageRefs(node) {
  if (node.getAttribute?.("style") && CSS_URL.test(node.getAttribute("style"))) node.removeAttribute("style");
  node.removeAttribute?.("background");
  node.removeAttribute?.("poster");
}

function retargetSelector(selector) {
  return selector
    .replace(/:root(?![\w-])/gi, `.${MAIL_HTML_CLASS}`)
    .replace(ROOT_SELECTOR, (_, lead, tag) =>
      `${lead}.${tag.toLowerCase() === "html" ? MAIL_HTML_CLASS : MAIL_BODY_CLASS}`);
}

function stripUrlDeclarations(style) {
  for (const prop of [...style]) {
    if (CSS_URL.test(style.getPropertyValue(prop))) style.removeProperty(prop);
  }
}

// Mail CSS is written for a whole document, but here it lives in a shadow root
// with no <html>/<body>: retarget those selectors onto our wrappers, drop rules
// aimed at the shadow host (they could position it over the app), and drop
// @import plus remote resources when images are blocked.
function rewriteRules(rules, blockImages) {
  let out = "";
  for (const rule of rules) {
    if (rule instanceof CSSStyleRule) {
      if (HOST_SELECTOR.test(rule.selectorText)) continue;
      if (blockImages) stripUrlDeclarations(rule.style);
      out += `${retargetSelector(rule.selectorText)}{${rule.style.cssText}}\n`;
    } else if (rule instanceof CSSMediaRule) {
      out += `@media ${rule.media.mediaText}{${rewriteRules(rule.cssRules, blockImages)}}\n`;
    } else if (typeof CSSSupportsRule !== "undefined" && rule instanceof CSSSupportsRule) {
      out += `@supports ${rule.conditionText}{${rewriteRules(rule.cssRules, blockImages)}}\n`;
    } else if (typeof CSSLayerBlockRule !== "undefined" && rule instanceof CSSLayerBlockRule) {
      out += `@layer ${rule.name}{${rewriteRules(rule.cssRules, blockImages)}}\n`;
    } else if (typeof CSSContainerRule !== "undefined" && rule instanceof CSSContainerRule) {
      out += `@container ${rule.conditionText}{${rewriteRules(rule.cssRules, blockImages)}}\n`;
    } else if (rule instanceof CSSImportRule) {
      continue;
    } else if (rule instanceof CSSKeyframesRule || !("cssRules" in rule)) {
      // Leaf at-rules (@font-face, @keyframes, ...); other grouping rules are dropped
      // since their contents would bypass the rewrite above.
      if (HOST_SELECTOR.test(rule.cssText) || (blockImages && CSS_URL.test(rule.cssText))) continue;
      out += `${rule.cssText}\n`;
    }
  }
  return out;
}

function rewriteCss(text, blockImages) {
  try {
    const sheet = new CSSStyleSheet();
    sheet.replaceSync(text);
    return rewriteRules(sheet.cssRules, blockImages);
  } catch {
    if (HOST_SELECTOR.test(text) || (blockImages && CSS_URL.test(text))) return "";
    return retargetSelector(text.replace(/@import[^;]*;/gi, ""));
  }
}

// Turns the sanitized document into { css, body }: all <style> contents
// (rewritten) and a <div> standing in for <body>, carrying its attributes.
function toShadowContent(root, blockImages) {
  const doc = root.ownerDocument;
  const body = root.querySelector("body") ?? root;

  let css = "";
  for (const style of root.querySelectorAll("style")) {
    css += rewriteCss(style.textContent || "", blockImages);
    style.remove();
  }

  const wrapper = doc.createElement("div");
  for (const { name, value } of body.attributes ?? []) wrapper.setAttribute(name, value);
  wrapper.classList.add(MAIL_BODY_CLASS);
  const bgcolor = body.getAttribute?.("bgcolor");
  if (bgcolor && !wrapper.style.backgroundColor) wrapper.style.backgroundColor = bgcolor;
  const text = body.getAttribute?.("text");
  if (text && !wrapper.style.color) wrapper.style.color = text;
  const background = body.getAttribute?.("background");
  if (background && !blockImages && !wrapper.style.backgroundImage) {
    wrapper.style.backgroundImage = `url(${JSON.stringify(background)})`;
  }
  wrapper.append(...body.childNodes);

  const only = wrapper.firstElementChild;
  if (wrapper.childElementCount === 1 && only.tagName === "PRE" && wrapper.textContent.trim() === only.textContent.trim()) {
    only.classList.add(MAIL_PLAIN_CLASS);
  }

  return { css, body: wrapper };
}

// Returns nodes, not a string: CSS text is only safe to place via textContent,
// and re-serializing sanitized markup invites mutation XSS.
export function sanitizeEmailHtml(html, { blockImages = false } = {}) {
  const config = {
    WHOLE_DOCUMENT: true,
    RETURN_DOM: true,
    FORBID_TAGS: blockImages ? [...FORM_TAGS, ...REMOTE_MEDIA_TAGS] : FORM_TAGS,
    FORBID_ATTR: ["formaction", "ping"],
  };
  if (blockImages) DOMPurify.addHook("afterSanitizeAttributes", stripImageRefs);
  try {
    const root = DOMPurify.sanitize(html || "", config);
    if (!root) return { css: "", body: document.createElement("div") };
    return toShadowContent(root, blockImages);
  } finally {
    if (blockImages) DOMPurify.removeHook("afterSanitizeAttributes", stripImageRefs);
  }
}

export function sanitizeEmailHtmlForCompose(html) {
  if (!html) return "";
  return DOMPurify.sanitize(html, {
    WHOLE_DOCUMENT: false,
    FORBID_TAGS: ["style", ...FORM_TAGS],
  });
}
