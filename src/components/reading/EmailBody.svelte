<script>
  import { openExternalUrl } from "../../lib/api.js";
  import { escapeHtml } from "../../lib/format.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { MAIL_HTML_CLASS, MAIL_PLAIN_CLASS, hasEmailImages, sanitizeEmailHtml } from "../../lib/sanitize.js";
  import { prefs, saveAppPrefs } from "../../state/prefs.svelte.js";

  const XLINK_NS = "http://www.w3.org/1999/xlink";

  // `contain` on the host keeps fixed/absolute-positioned mail content from
  // escaping over the app; mail CSS can't target :host (see sanitize.js).
  // The wrapper resets everything inherited from the app (font, line-height,
  // colors) so mails render on a neutral "paper" like in webmail clients.
  const STYLE = `
    :host { display: block; contain: content !important; }
    .${MAIL_HTML_CLASS} {
      all: initial;
      display: block;
      overflow-x: auto;
      padding: 16px;
      border-radius: 10px;
      background: #fff;
      color: #222;
      font-family: Arial, Helvetica, sans-serif;
      font-size: 14px;
      line-height: normal;
      overflow-wrap: break-word;
      -webkit-user-select: text;
      user-select: text;
      cursor: auto;
    }
    pre { white-space: pre-wrap; overflow-wrap: anywhere; }
    pre.${MAIL_PLAIN_CLASS} { margin: 0; font-family: inherit; font-size: inherit; }
    a { cursor: pointer; }
  `;

  let { email, class: className = "" } = $props();

  let loadedFor = $state(null);

  const source = $derived(email.body_html || `<pre>${escapeHtml(email.snippet || "")}</pre>`);
  const blockImages = $derived(prefs.app.loadImages === false && loadedFor !== email.id && hasEmailImages(source));
  const content = $derived(sanitizeEmailHtml(source, { blockImages }));

  function openLink(event) {
    const target = event.target instanceof Element ? event.target : event.target?.parentElement;
    const anchor = target?.closest("a, area");
    if (!anchor) return;
    event.preventDefault();
    event.stopPropagation();
    const href = anchor.dataset.href || "";
    if (/^https?:\/\//i.test(href)) openExternalUrl(href).catch((error) => console.error("Failed to open link", error));
  }

  function emailContent(host) {
    const shadow = host.attachShadow({ mode: "closed" });
    shadow.addEventListener("click", openLink, true);
    shadow.addEventListener("auxclick", openLink, true);
    shadow.addEventListener("keydown", (event) => event.key === "Enter" && openLink(event), true);

    $effect(() => {
      const base = document.createElement("style");
      base.textContent = STYLE;
      const mailStyle = document.createElement("style");
      mailStyle.textContent = content.css;
      const root = document.createElement("div");
      root.className = MAIL_HTML_CLASS;
      root.append(document.importNode(content.body, true));

      // Links never navigate the webview; the real target is opened externally.
      for (const anchor of root.querySelectorAll("a, area")) {
        anchor.dataset.href = (anchor.getAttribute("href") || anchor.getAttributeNS(XLINK_NS, "href") || "").trim();
        anchor.removeAttributeNS(XLINK_NS, "href");
        anchor.removeAttribute("target");
        anchor.setAttribute("href", "#");
        anchor.setAttribute("rel", "noopener noreferrer");
      }

      shadow.replaceChildren(base, mailStyle, root);
    });
  }
</script>

{#if blockImages}
  <div class="email-images-banner">
    <span>{t("reading.images_blocked")}</span>
    <button class="verdant-btn" onclick={() => (loadedFor = email.id)}>{t("reading.images_load")}</button>
    <button class="verdant-btn" onclick={() => saveAppPrefs({ loadImages: true })}>{t("reading.images_always")}</button>
  </div>
{/if}
<div class={className} {@attach emailContent}></div>
