<script>
  import { openExternalUrl } from "../../lib/api.js";
  import { escapeHtml, sanitizeUnicodeNoise } from "../../lib/format.js";
  import { sanitizeEmailHtml } from "../../lib/sanitize.js";

  const STYLE = `
    :host { display: block; overflow-x: auto; -webkit-user-select: text; user-select: text; }
    * { box-sizing: border-box; }
    .email-center { max-width: 640px; margin: 0 auto; }
    p { margin-bottom: 12px; }
    p:last-child { margin-bottom: 0; }
    pre {
      white-space: pre-wrap;
      word-break: break-word;
      background: var(--surface, #f0f0ec);
      border: 1px solid var(--border, #d6d9d2);
      border-radius: 8px;
      padding: 10px 12px;
      font-size: 12px;
    }
    img { max-width: 100%; height: auto; }
    a { color: var(--green, #4a5e45); }
    table { max-width: 100%; overflow-x: auto; display: block; }
  `;

  let { email, class: className = "" } = $props();

  const html = $derived(
    sanitizeEmailHtml(sanitizeUnicodeNoise(email.body_html || `<pre>${escapeHtml(email.snippet || "")}</pre>`)),
  );

  function openLink(event) {
    const anchor = (event.target instanceof Element ? event.target : event.target?.parentElement)?.closest("a[href]");
    if (!anchor) return;
    event.preventDefault();
    event.stopPropagation();
    const href = anchor.dataset.href || "";
    if (/^https?:\/\//.test(href)) openExternalUrl(href).catch((error) => console.error("Failed to open link", error));
  }

  function emailContent(host) {
    const shadow = host.attachShadow({ mode: "closed" });
    shadow.addEventListener("click", openLink, true);
    shadow.addEventListener("auxclick", openLink, true);
    shadow.addEventListener("keydown", (event) => event.key === "Enter" && openLink(event), true);

    $effect(() => {
      shadow.innerHTML = `<style>${STYLE}</style><div class="email-center">${html}</div>`;
      for (const anchor of shadow.querySelectorAll("a[href]")) {
        anchor.dataset.href = anchor.getAttribute("href") || "";
        anchor.setAttribute("href", "#");
        anchor.setAttribute("target", "_self");
        anchor.setAttribute("rel", "noopener noreferrer");
      }
    });
  }
</script>

<div class={className} {@attach emailContent}></div>
