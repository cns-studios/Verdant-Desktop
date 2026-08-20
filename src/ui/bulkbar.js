import { archiveEmail, trashEmail, toggleStarred, permanentDeleteEmail, getThreadMessages } from "../api.js";
import { showToast } from "../lib/toast.js";
import { t } from "../lib/i18n.js";
import { refreshCounts } from "./sidebar.js";
import {
    subscribe, getMasterState, getSelectedRows, selectionCount,
    isActive, selectAllVisible, deselectAllVisible, exitMultiSelect,
} from "./multiselect.js";

let ctx = null;

async function resolveIds(row) {
    if (row.dataset.threadId) {
        try {
            const messages = await getThreadMessages(row.dataset.threadId);
            return messages.map((m) => m.id);
        } catch {
            return [];
        }
    }
    return row.dataset.emailId ? [row.dataset.emailId] : [];
}

function resolveTarget(row) {
    if (row.dataset.threadId) return ctx.resolveThread(row.dataset.threadId);
    return ctx.resolveEmail(row.dataset.emailId);
}

function updateBulkBar() {
    const bar = document.getElementById("bulk-bar");
    const master = document.getElementById("bulk-master");
    if (!bar || !master) return;

    const filterBar = document.querySelector(".filter-bar");
    filterBar?.classList.toggle("bulk-mode", isActive());

    const state = getMasterState();
    master.classList.toggle("state-all", state === "all");
    master.classList.toggle("state-some", state === "some");
    master.classList.toggle("state-none", state === "none");
    master.title = state === "all" ? t("bulk.deselect_all") : t("bulk.select_all");

    bar.classList.toggle("empty", selectionCount() === 0);

    const rows = getSelectedRows();
    const allStarred = rows.length > 0 && rows.every((row) => {
        const target = resolveTarget(row);
        return target && target.starred;
    });
    bar.querySelector('[data-bulk="star"]')?.classList.toggle("active", allStarred && rows.length > 0);
}

async function runPerSelectedRow(fn) {
    const rows = getSelectedRows();
    await Promise.all(rows.map((row) => fn(row).catch(() => {})));
}

async function bulkArchive() {
    if (selectionCount() === 0) return;
    await runPerSelectedRow(async (row) => {
        const ids = await resolveIds(row);
        await Promise.all(ids.map((id) => archiveEmail(id)));
    });
    showToast(t("toast.archived"));
    await refreshAfterBulk();
}

async function bulkDelete() {
    if (selectionCount() === 0) return;
    const mailbox = (ctx.getMailbox() || "INBOX").toUpperCase();
    const isTrash = mailbox.includes("TRASH");
    await runPerSelectedRow(async (row) => {
        const ids = await resolveIds(row);
        await Promise.all(ids.map((id) => (isTrash ? permanentDeleteEmail(id) : trashEmail(id))));
    });
    showToast(isTrash ? t("toast.permanently_deleted") : t("toast.trashed"));
    await refreshAfterBulk();
}

async function bulkStar() {
    if (selectionCount() === 0) return;
    const rows = getSelectedRows();
    const targets = rows.map((row) => resolveTarget(row)).filter(Boolean);
    const allStarred = targets.length > 0 && targets.every((target) => target.starred);
    const next = !allStarred;

    await runPerSelectedRow(async (row) => {
        const target = resolveTarget(row);
        if (target) target.starred = next;
        const ids = await resolveIds(row);
        if (target && target.starred === next) return;
        await Promise.all(ids.map((id) => toggleStarred(id)));
    });
    refreshCounts().catch(() => {});
    showToast(t("toast.star_updated"));

    const mailbox = (ctx.getMailbox() || "INBOX").toUpperCase();
    if (mailbox === "STARRED") {
        await refreshAfterBulk();
    } else {
        updateBulkBar();
    }
}

async function refreshAfterBulk() {
    exitMultiSelect();
    await ctx.onRefresh();
}

export function bindBulkBar(c) {
    ctx = c;
    const bar = document.getElementById("bulk-bar");
    const master = document.getElementById("bulk-master");
    if (!bar || !master) return;

    subscribe(updateBulkBar);
    updateBulkBar();

    master.addEventListener("click", () => {
        if (getMasterState() === "all") {
            deselectAllVisible();
        } else {
            selectAllVisible();
        }
    });

    bar.querySelectorAll("[data-bulk]").forEach((btn) => {
        btn.addEventListener("click", () => {
            const action = btn.dataset.bulk;
            if (action === "archive") void bulkArchive();
            else if (action === "delete") void bulkDelete();
            else if (action === "star") void bulkStar();
        });
    });
}