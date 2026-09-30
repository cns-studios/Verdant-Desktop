import { getInboxCategories, renameInboxCategory } from "../api.js";
import { escapeHtml } from "../lib/format.js";
import { t } from "../lib/i18n.js";
import { categoryLabel, refreshSmartInboxEnabled, setSmartInboxEnabled } from "../lib/smartInbox.js";
import { icon } from "./icons.js";

/**
 * Smart Inbox settings: an on/off switch, the account's groups with their
 * size and inline rename, and "Sort again". Re-renders itself after changes.
 * `openSetup(resort)` must close Settings and open the setup dialog.
 */
export function mountSmartInboxSettings(container, { accountEmail = "", showAccount = false, openSetup }) {
    let enabled = false;
    let categories = [];

    const groupRow = (c) => {
        const meta = c.unread_count
            ? t("settings.smart.group_meta_unread", { n: c.message_count, unread: c.unread_count })
            : t("settings.smart.group_meta", { n: c.message_count });
        return `
        <li class="sis-row" data-slug="${escapeHtml(c.slug)}" style="--group-color:${escapeHtml(c.color || "#7b8075")}">
            <span class="sis-dot"></span>
            <span class="sis-row-name">${escapeHtml(categoryLabel(c))}</span>
            <span class="sis-row-meta">${escapeHtml(meta)}</span>
            <button class="sis-icon-btn" data-rename aria-label="${escapeHtml(t("smart.rename"))}" title="${escapeHtml(t("smart.rename"))}">${icon("pencil")}</button>
        </li>`;
    };

    function render() {
        const sorted = enabled && categories.length > 0;
        container.innerHTML = `
            <div class="sis-settings-head">
                <div>
                    <h3>${escapeHtml(t("settings.smart.title"))}</h3>
                    <p>${escapeHtml(t("settings.smart.description"))}</p>
                    ${showAccount && accountEmail ? `<p class="sis-for">${escapeHtml(t("settings.smart.for_account", { email: accountEmail }))}</p>` : ""}
                </div>
                <label class="settings-switch sis-switch" title="${escapeHtml(t("settings.smart.title"))}">
                    <input id="settings-smart-enabled" type="checkbox" ${enabled ? "checked" : ""} aria-label="${escapeHtml(t("settings.smart.title"))}">
                </label>
            </div>
            ${sorted ? `
                <div class="settings-section-label">${escapeHtml(t("settings.smart.groups"))}</div>
                <ul class="sis-rows">${categories.map(groupRow).join("")}</ul>
                <p class="settings-help">${escapeHtml(t("settings.smart.groups_help"))}</p>
                <div class="sis-resort">
                    <div>
                        <strong>${escapeHtml(t("settings.smart.resort_title"))}</strong>
                        <p>${escapeHtml(t("settings.smart.resort_body"))}</p>
                    </div>
                    <button class="verdant-btn" data-action="resort">${escapeHtml(t("settings.smart.resort_button"))}</button>
                </div>` : ""}
            ${enabled && !sorted ? `
                <div class="sis-empty">
                    <p>${escapeHtml(t("settings.smart.not_sorted"))}</p>
                    <button class="verdant-btn primary" data-action="setup">${escapeHtml(t("smart.setup.sort"))}</button>
                </div>` : ""}`;
        bind();
    }

    function bind() {
        container.querySelector("#settings-smart-enabled").addEventListener("change", async (e) => {
            if (e.target.checked) {
                // Turning it on goes through the setup dialog, which shows the
                // groups first; nothing changes until the user confirms there.
                e.target.checked = false;
                openSetup(false);
                return;
            }
            if (!confirm(t("settings.smart.confirm_off"))) {
                e.target.checked = true;
                return;
            }
            try {
                await setSmartInboxEnabled(false);
                await load();
            } catch (err) {
                console.error("Failed to turn off smart inbox", err);
                e.target.checked = true;
            }
        });
        container.querySelector('[data-action="resort"]')?.addEventListener("click", () => openSetup(true));
        container.querySelector('[data-action="setup"]')?.addEventListener("click", () => openSetup(false));
        container.querySelectorAll("[data-rename]").forEach(btn => btn.addEventListener("click", () => startRename(btn.closest(".sis-row"))));
        container.querySelectorAll(".sis-row-name").forEach(el => el.addEventListener("dblclick", () => startRename(el.closest(".sis-row"))));
    }

    function startRename(row) {
        const category = categories.find(c => c.slug === row.dataset.slug);
        if (!category || row.classList.contains("editing")) return;
        row.classList.add("editing");
        const name = row.querySelector(".sis-row-name");
        const current = categoryLabel(category);
        name.innerHTML = `<input class="sis-rename" maxlength="40" value="${escapeHtml(current)}" aria-label="${escapeHtml(t("smart.rename"))}">`;
        const input = name.querySelector("input");
        input.focus();
        input.select();
        let done = false;
        const finish = async (save) => {
            if (done) return;
            done = true;
            const next = input.value.trim();
            if (save && next && next !== current) {
                try {
                    await renameInboxCategory(category.slug, next);
                    window.dispatchEvent(new CustomEvent("smart-inbox-changed"));
                } catch (err) {
                    console.error("Failed to rename group", err);
                }
            }
            await load();
        };
        input.addEventListener("keydown", e => {
            if (e.key === "Enter") finish(true);
            if (e.key === "Escape") { e.stopPropagation(); finish(false); }
        });
        input.addEventListener("blur", () => finish(true));
    }

    async function load() {
        try {
            enabled = await refreshSmartInboxEnabled();
            categories = enabled ? await getInboxCategories() : [];
        } catch (err) {
            console.error("Failed to load smart inbox settings", err);
        }
        render();
    }

    return load();
}
