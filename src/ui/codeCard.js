import { escapeHtml } from "../lib/format.js";
import { t } from "../lib/i18n.js";
import { showToast } from "../lib/toast.js";
import { formatCode } from "../lib/verificationCode.js";
import { icon } from "./icons.js";

const COPIED_MS = 1800;

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // Some webviews refuse the async clipboard; fall back to a selection.
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.cssText = "position:fixed;opacity:0;pointer-events:none";
    document.body.appendChild(area);
    area.select();
    let ok = false;
    try { ok = document.execCommand("copy"); } catch {}
    area.remove();
    return ok;
  }
}

function bindCopy(root, button, code) {
  let timer = null;
  button.addEventListener("click", async (e) => {
    // In the list the chip sits inside a row that opens the mail on click.
    e.stopPropagation();
    if (!(await copyText(code))) {
      showToast(t("code.copy_failed"), "error");
      return;
    }
    root.classList.add("copied");
    button.querySelector(".vc-copy-text").textContent = t("code.copied");
    clearTimeout(timer);
    timer = setTimeout(() => {
      root.classList.remove("copied");
      button.querySelector(".vc-copy-text").textContent = t("code.copy");
    }, COPIED_MS);
  });
}

const copyButtonHtml = () => `
  <span class="vc-copy-icon">${icon("copy")}${icon("check")}</span>
  <span class="vc-copy-text">${escapeHtml(t("code.copy"))}</span>`;

/** The card above an opened mail: the code is typed out digit by digit. */
export function buildCodeCard(code) {
  const card = document.createElement("div");
  card.className = "vc-card";
  const digits = [...code].map((d, i) => `<span class="vc-digit" style="--i:${i}">${d}</span>`).join("");
  card.innerHTML = `
    <span class="vc-badge">${icon("shield-check")}</span>
    <div class="vc-main">
      <span class="vc-label">${escapeHtml(t("code.label"))}</span>
      <span class="vc-digits" role="text" aria-label="${escapeHtml(formatCode(code))}">
        <span aria-hidden="true">${digits}<span class="vc-caret"></span></span>
      </span>
    </div>
    <button type="button" class="vc-copy">${copyButtonHtml()}</button>`;
  bindCopy(card, card.querySelector(".vc-copy"), code);
  return card;
}

/** The compact version under a mail's preview line in the list. */
export function buildCodeChip(code, animate = true) {
  const chip = document.createElement("div");
  chip.className = `vc-chip${animate ? " vc-enter" : ""}`;
  chip.innerHTML = `
    <span class="vc-chip-code" title="${escapeHtml(t("code.label"))}">${icon("shield-check")}${escapeHtml(formatCode(code))}</span>
    <button type="button" class="vc-chip-copy">${copyButtonHtml()}</button>`;
  bindCopy(chip, chip.querySelector(".vc-chip-copy"), code);
  return chip;
}
