import DOMPurify from "dompurify";

export function sanitizeEmailHtml(html) {
  if (!html) return "";
  return DOMPurify.sanitize(html, {
    WHOLE_DOCUMENT: false,
    ADD_ATTR: ["target"],
  });
}

export function sanitizeEmailHtmlForCompose(html) {
  if (!html) return "";
  return DOMPurify.sanitize(html, {
    WHOLE_DOCUMENT: false,
    FORBID_TAGS: ["style"],
  });
}
