import { listAccounts, switchAccount, removeAccount } from "../api.js";
import { openWhatsNewModal } from "./whatsnew.js";
import { showOnboarding } from "./onboarding.js";
import { detectProvider } from "../lib/mailProviders.js";
import { escapeHtml } from "../lib/format.js";
import { showToast } from "../lib/toast.js";
import { t } from "../lib/i18n.js";
import { icon } from "./icons.js";



let _popoverOpen = false;

export function isAccountPopoverOpen() { return _popoverOpen; }

export async function openAccountPopover(onSwitch, onAddAccount) {
    closeAccountPopover();

    let accounts = [];
    try { accounts = await listAccounts(); } catch {}

    _popoverOpen = true;

    
    const backdrop = document.createElement("div");
    backdrop.className = "account-popover-backdrop";
    backdrop.onclick = closeAccountPopover;
    document.body.appendChild(backdrop);

    
    const pop = document.createElement("div");
    pop.className = "account-popover";
    pop.id = "account-popover";

    
    const accSection = document.createElement("div");
    accSection.className = "account-popover-section";

    if (accounts.length > 0) {
        const label = document.createElement("div");
        label.className = "account-popover-label";
        label.textContent = t("sidebar.accounts");
        accSection.appendChild(label);

        for (const acc of accounts) {
            const item = document.createElement("div");
            item.className = `account-item${acc.is_active ? " is-active" : ""}`;

            const initials = (acc.display_name || acc.email).slice(0, 2).toUpperCase();
            item.innerHTML = `
                <div class="account-avatar ${acc.provider === 'imap' ? 'imap' : ''}">${escapeHtml(initials)}</div>
                <div class="account-item-info">
                    <div class="account-item-email" title="${escapeHtml(acc.email)}">${escapeHtml(acc.email)}</div>
                    <div class="account-item-provider">${escapeHtml(acc.provider === 'imap' ? detectProvider(acc.email).name : 'Gmail')}</div>
                </div>
                ${acc.is_active ? '<div class="account-active-dot"></div>' : ''}
                ${!acc.is_active ? `<button class="account-remove-btn" title="${t("reading.delete")}">×</button>` : ''}
            `;

            if (!acc.is_active) {
                item.addEventListener("click", async (e) => {
                    if (e.target.classList.contains("account-remove-btn")) return;
                    closeAccountPopover();
                    try {
                        await switchAccount(acc.id);
                        onSwitch(acc.id);
                    } catch (err) {
                        showToast(String(err), "error");
                    }
                });

                const removeBtn = item.querySelector(".account-remove-btn");
                removeBtn?.addEventListener("click", async (e) => {
                    e.stopPropagation();
                    if (!confirm(t("accounts.confirm_remove", { email: acc.email }))) return;
                    closeAccountPopover();
                    try {
                        await removeAccount(acc.id);
                        showToast(t("accounts.removed"));
                        onSwitch(null); 
                    } catch (err) {
                        showToast(String(err), "error");
                    }
                });
            }

            accSection.appendChild(item);
        }
    }

    pop.appendChild(accSection);

    
    const actSection = document.createElement("div");
    actSection.className = "account-popover-section";

    const addBtn = document.createElement("div");
    addBtn.className = "account-popover-action";
    addBtn.innerHTML = `
        ${icon("circle-plus")}
        ${t("sidebar.add_account")}
    `;
    addBtn.onclick = () => {
        closeAccountPopover();
        showOnboarding(onAddAccount, true);
    };

    actSection.appendChild(addBtn);

    const whatsNewBtn = document.createElement("div");
    whatsNewBtn.className = "account-popover-action";
    whatsNewBtn.innerHTML = `
        ${icon("news")}
        ${t("whatsnew.title")}
    `;
    whatsNewBtn.onclick = async () => {
        closeAccountPopover();
        try {
            const { invoke } = await import("@tauri-apps/api/core");
            const updateInfo = await invoke("check_for_updates");
            await openWhatsNewModal(updateInfo.currentVersion, true);
        } catch (err) {
            showToast(String(err), "error");
        }
    };
    actSection.appendChild(whatsNewBtn);

    const settingsBtn = document.createElement("div");
    settingsBtn.className = "account-popover-action";
    settingsBtn.innerHTML = `
        ${icon("settings")}
        ${t("settings.title")}
    `;
    settingsBtn.onclick = () => {
        closeAccountPopover();
        
        window.dispatchEvent(new CustomEvent("verdant-open-settings"));
    };
    actSection.appendChild(settingsBtn);
    
    pop.appendChild(actSection);

    document.body.appendChild(pop);
}

export function closeAccountPopover() {
    document.getElementById("account-popover")?.remove();
    document.querySelector(".account-popover-backdrop")?.remove();
    _popoverOpen = false;
}
