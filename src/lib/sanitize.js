import DOMPurify from "dompurify";

const IMAGE_HINT = /<(img|picture|source)\b|url\(|\sbackground\s*=/i;
const CSS_URL = /url\(/i;

export const hasEmailImages = (html) => IMAGE_HINT.test(html || "");

function stripImageRefs(node) {
  if (node.getAttribute?.("style") && CSS_URL.test(node.getAttribute("style"))) node.removeAttribute("style");
  node.removeAttribute?.("background");
}

export function sanitizeEmailHtml(html, { blockImages = false } = {}) {
  if (!html) return "";
  const config = { WHOLE_DOCUMENT: false, ADD_ATTR: ["target"] };
  if (!blockImages) return DOMPurify.sanitize(html, config);
  DOMPurify.addHook("afterSanitizeAttributes", stripImageRefs);
  try {
    return DOMPurify.sanitize(html, { ...config, FORBID_TAGS: ["img", "picture", "source", "style"] });
  } finally {
    DOMPurify.removeHook("afterSanitizeAttributes", stripImageRefs);
  }
}

export function sanitizeEmailHtmlForCompose(html) {
  if (!html) return "";
  return DOMPurify.sanitize(html, {
    WHOLE_DOCUMENT: false,
    FORBID_TAGS: ["style"],
  });
}
