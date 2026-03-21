import { connectGmail } from "../api.js";
import { escapeHtml } from "../lib/format.js";
import { showToast } from "../lib/toast.js";

function providerCardHtml(id, label, description, icon, available) {
  const disabledAttrs = available ? "" : `style="cursor:not-allowed; opacity:0.52;"`;
  const comingSoon = available ? "" : `<span class="ob-coming-soon">Coming soon</span>`;
  return `
    <div class="ob-provider-card ${available ? "ob-available" : "ob-unavailable"}" data-provider="${id}" ${disabledAttrs}>
      <div class="ob-provider-icon">${icon}</div>
      <div class="ob-provider-info">
        <div class="ob-provider-label">${escapeHtml(label)}</div>
        <div class="ob-provider-desc">${escapeHtml(description)}</div>
      </div>
      ${comingSoon}
    </div>
  `;
}

function buildOnboardingStyles() {
  if (document.getElementById("verdant-onboarding-styles")) return;
  const style = document.createElement("style");
  style.id = "verdant-onboarding-styles";
  style.textContent = `
    .ob-root {
      position: fixed;
      inset: 0;
      background: var(--bg);
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      z-index: 9000;
      padding: 24px;
    }

    .ob-inner {
      width: min(480px, 100%);
      display: flex;
      flex-direction: column;
      gap: 28px;
    }

    .ob-brand {
      display: flex;
      align-items: center;
      gap: 11px;
    }

    .ob-logo-mark {
      width: 34px;
      height: 34px;
      background: var(--green);
      border-radius: 9px;
      display: flex;
      align-items: center;
      justify-content: center;
      flex-shrink: 0;
    }

    .ob-logo-mark svg {
      width: 17px;
      height: 17px;
    }

    .ob-brand-name {
      font: 500 20px 'Fraunces', serif;
      color: var(--text);
      letter-spacing: -0.3px;
    }

    .ob-heading {
      display: flex;
      flex-direction: column;
      gap: 6px;
    }

    .ob-heading h1 {
      font: 400 28px/1.15 'Fraunces', serif;
      color: var(--text);
      letter-spacing: -0.5px;
      margin: 0;
    }

    .ob-heading p {
      font: 400 13px 'DM Sans', sans-serif;
      color: var(--text-muted);
      margin: 0;
      line-height: 1.5;
    }

    .ob-providers {
      display: flex;
      flex-direction: column;
      gap: 10px;
    }

    .ob-provider-card {
      display: flex;
      align-items: center;
      gap: 14px;
      padding: 14px 16px;
      border: 1px solid var(--border);
      border-radius: 11px;
      background: var(--white);
      transition: border-color 0.12s, box-shadow 0.12s;
      position: relative;
    }

    .ob-available {
      cursor: pointer;
    }

    .ob-available:hover {
      border-color: var(--green-light);
      box-shadow: 0 2px 10px rgba(74,94,69,0.09);
    }

    .ob-unavailable {
      background: var(--surface);
    }

    .ob-provider-icon {
      width: 36px;
      height: 36px;
      border-radius: 8px;
      background: var(--surface2);
      border: 1px solid var(--border);
      display: flex;
      align-items: center;
      justify-content: center;
      flex-shrink: 0;
      font-size: 18px;
    }

    .ob-provider-info {
      flex: 1;
      min-width: 0;
    }

    .ob-provider-label {
      font: 500 13.5px 'DM Sans', sans-serif;
      color: var(--text);
      margin-bottom: 2px;
    }

    .ob-provider-desc {
      font: 400 12px 'DM Sans', sans-serif;
      color: var(--text-muted);
    }

    .ob-coming-soon {
      font: 500 10px 'DM Sans', sans-serif;
      color: var(--text-muted);
      background: var(--surface2);
      border: 1px solid var(--border);
      border-radius: 999px;
      padding: 2px 8px;
      white-space: nowrap;
      flex-shrink: 0;
    }

    .ob-error {
      font: 400 12px 'DM Sans', sans-serif;
      color: #8a3b3b;
      background: #f9ecec;
      border: 1px solid #dcb9b9;
      border-radius: 8px;
      padding: 10px 12px;
      display: none;
    }

    .ob-error.visible {
      display: block;
    }
  `;
  document.head.appendChild(style);
}

export function showOnboarding(onSuccess, errorMessage = "") {
  buildOnboardingStyles();

  document.getElementById("verdant-onboarding")?.remove();

  const root = document.createElement("div");
  root.id = "verdant-onboarding";
  root.className = "ob-root";
  root.innerHTML = `
    <div class="ob-inner">
      <div class="ob-brand">
        <div class="ob-logo-mark">
          <svg viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z"/>
            <polyline points="22,6 12,13 2,6"/>
          </svg>
        </div>
        <span class="ob-brand-name">Verdant</span>
      </div>

      <div class="ob-heading">
        <h1>Connect your inbox</h1>
        <p>Choose a mail provider to get started. Your credentials stay on your device.</p>
      </div>

      <div class="ob-providers">
        ${providerCardHtml(
          "gmail",
          "Gmail",
          "Sign in with Google OAuth — no password stored",
          `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" width="20" height="20"><path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z"/><polyline points="22,6 12,13 2,6"/></svg>`,
          true
        )}
        ${providerCardHtml(
          "outlook",
          "Outlook",
          "Connect your Microsoft or Outlook.com account",
          `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" width="20" height="20"><rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8M12 17v4"/></svg>`,
          false
        )}
        ${providerCardHtml(
          "gmx",
          "GMX",
          "Connect via SMTP with your GMX credentials",
          `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" width="20" height="20"><circle cx="12" cy="12" r="10"/><path d="M2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/></svg>`,
          false
        )}
        ${providerCardHtml(
          "smtp",
          "Custom SMTP",
          "Enter your own SMTP server details",
          `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" width="20" height="20"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/></svg>`,
          false
        )}
      </div>

      <div class="ob-error ${errorMessage ? "visible" : ""}" id="ob-error">
        ${escapeHtml(errorMessage)}
      </div>
    </div>
  `;

  const gmailCard = root.querySelector('[data-provider="gmail"]');
  gmailCard?.addEventListener("click", async () => {
    gmailCard.style.opacity = "0.6";
    gmailCard.style.pointerEvents = "none";
    gmailCard.querySelector(".ob-provider-label").textContent = "Connecting...";

    const errorEl = root.querySelector("#ob-error");

    try {
      await connectGmail();
      root.remove();
      onSuccess();
    } catch (err) {
      gmailCard.style.opacity = "";
      gmailCard.style.pointerEvents = "";
      gmailCard.querySelector(".ob-provider-label").textContent = "Gmail";
      if (errorEl) {
        errorEl.textContent = String(err);
        errorEl.classList.add("visible");
      }
    }
  });

  document.getElementById("root").appendChild(root);
}

export function hideOnboarding() {
  document.getElementById("verdant-onboarding")?.remove();
}
