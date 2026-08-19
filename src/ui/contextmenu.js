import {
    getThreadMessages,
    archiveEmail, trashEmail, toggleStarred, setEmailReadStatus,
    restoreFromTrash, moveToInbox, permanentDeleteEmail,
} from "../api.js";
import { showToast } from "../lib/toast.js";
import { t } from "../lib/i18n.js";
import { icon } from "./icons.js";
import { refreshCounts } from "./sidebar.js";

let _menu = null;

function closeMenu() {
    if (!_menu) return;
    _menu.remove();
    _menu = null;
    document.removeEventListener("click", onDocClick, true);
    document.removeEventListener("keydown", onDocKeydown, true);
    window.removeEventListener("scroll", onDocScroll, true);
    window.removeEventListener("blur", closeMenu);
}

function onDocClick(event) {
    if (_menu && !_menu.contains(event.target)) closeMenu();
}

function onDocKeydown(event) {
    if (event.key === "Escape") closeMenu();
}

function onDocScroll() {
    closeMenu();
}

function resolveTargetForRefresh(ctx, row) {
    if (row.dataset.threadId) return ctx.resolveThread(row.dataset.threadId);
    if (row.dataset.emailId) return ctx.resolveEmail(row.dataset.emailId);
    return null;
}

async function resolveMessageIds(ctx, row) {
    const threadId = row.dataset.threadId;
    if (threadId) {
        try {
            const messages = await getThreadMessages(threadId);
            return messages.map((m) => m.id);
        } catch {
            return [];
        }
    }
    const emailId = row.dataset.emailId;
    return emailId ? [emailId] : [];
}

function buildMenu(x, y, items) {
    closeMenu();

    const menu = document.createElement("div");
    menu.className = "email-context-menu";
    menu.id = "email-context-menu";
    menu.setAttribute("role", "menu");

    for (const item of items) {
        if (item.separator) {
            const sep = document.createElement("div");
            sep.className = "email-context-sep";
            menu.appendChild(sep);
            continue;
        }
        const btn = document.createElement("button");
        btn.className = "email-context-item";
        if (item.danger) btn.classList.add("danger");
        btn.setAttribute("role", "menuitem");
        btn.innerHTML = `${icon(item.icon)}<span>${item.label}</span>`;
        btn.onclick = (event) => {
            event.stopPropagation();
            closeMenu();
            item.onClick();
        };
        menu.appendChild(btn);
    }

    document.body.appendChild(menu);

    const rect = menu.getBoundingClientRect();
    const pad = 8;
    let left = x;
    let top = y;
    if (left + rect.width + pad > window.innerWidth) left = Math.max(pad, window.innerWidth - rect.width - pad);
    if (top + rect.height + pad > window.innerHeight) top = Math.max(pad, window.innerHeight - rect.height - pad);
    menu.style.left = `${left}px`;
    menu.style.top = `${top}px`;

    _menu = menu;
    setTimeout(() => {
        document.addEventListener("click", onDocClick, true);
        document.addEventListener("keydown", onDocKeydown, true);
        window.addEventListener("scroll", onDocScroll, true);
        window.addEventListener("blur", closeMenu);
    }, 0);
}

function removedThreadId(row) {
    return row.dataset.threadId || null;
}

async function actionArchive(ctx, row) {
    const ids = await resolveMessageIds(ctx, row);
    await Promise.all(ids.map((id) => archiveEmail(id).catch(() => {})));
    showToast(t("toast.archived"));
    await ctx.onRefresh(ids, null, null, removedThreadId(row));
}

async function actionRestore(ctx, row) {
    const ids = await resolveMessageIds(ctx, row);
    await Promise.all(ids.map((id) => restoreFromTrash(id).catch(() => {})));
    showToast(t("toast.restored"));
    await ctx.onRefresh(ids, "INBOX", resolveTargetForRefresh(ctx, row), removedThreadId(row));
}

async function actionMoveToInbox(ctx, row) {
    const ids = await resolveMessageIds(ctx, row);
    await Promise.all(ids.map((id) => moveToInbox(id).catch(() => {})));
    showToast(t("toast.moved_to_inbox"));
    await ctx.onRefresh(ids, "INBOX", resolveTargetForRefresh(ctx, row), removedThreadId(row));
}

async function actionDelete(ctx, row) {
    const ids = await resolveMessageIds(ctx, row);
    const mailbox = (ctx.getMailbox() || "INBOX").toUpperCase();
    if (mailbox.includes("TRASH")) {
        await Promise.all(ids.map((id) => permanentDeleteEmail(id).catch(() => {})));
        showToast(t("toast.permanently_deleted"));
    } else {
        await Promise.all(ids.map((id) => trashEmail(id).catch(() => {})));
        showToast(t("toast.trashed"));
    }
    await ctx.onRefresh(ids, null, null, removedThreadId(row));
}

async function actionMarkUnread(ctx, row) {
    const ids = await resolveMessageIds(ctx, row);
    await Promise.all(ids.map((id) => setEmailReadStatus(id, false).catch(() => {})));
    const target = resolveTargetForRefresh(ctx, row);
    if (target) target.is_read = false;
    showToast(t("toast.unread_marked"));
    await ctx.onRefresh();
}

function updateStarBadge(row, starred) {
    if (!row) return;
    let badge = row.querySelector(".star-badge");
    if (starred) {
        if (!badge) {
            badge = document.createElement("span");
            badge.className = "star-badge";
            row.querySelector(".email-item-inner")?.appendChild(badge);
        } else {
            badge.classList.remove("pop");
            void badge.offsetWidth;
        }
        badge.classList.add("pop");
        badge.innerHTML = icon("star-filled", 18);
    } else if (badge) {
        badge.remove();
    }
}

function actionToggleStar(ctx, row) {
    const target = resolveTargetForRefresh(ctx, row);
    const mailbox = (ctx.getMailbox() || "INBOX").toUpperCase();
    const next = target ? !target.starred : false;
    if (target) target.starred = next;
    updateStarBadge(row, next);
    refreshCounts().catch(() => {});
    showToast(t("toast.star_updated"));

    const threadId = row.dataset.threadId;
    const emailId = row.dataset.emailId;
    if (threadId) {
        resolveMessageIds(ctx, row)
            .then((ids) => Promise.all(ids.map((id) => toggleStarred(id).catch(() => {}))))
            .catch(() => {});
    } else if (emailId) {
        toggleStarred(emailId).catch(() => {});
    }

    if (mailbox === "STARRED" && emailId) {
        void ctx.onRefresh([emailId]);
    }
}

async function openMenu(row, x, y, ctx) {
    const mailbox = (ctx.getMailbox() || "INBOX").toUpperCase();
    const isTrash = mailbox.includes("TRASH");
    const isArchive = mailbox.includes("ARCHIVE");
    const isSent = mailbox.includes("SENT");
    const isDraft = mailbox.includes("DRAFT");

    const items = [];

    const target = resolveTargetForRefresh(ctx, row);
    const isStarred = !!target?.starred;

    if (!isDraft && !isSent) {
        if (isTrash) {
            items.push({ icon: "rotate", label: t("reading.restore"), onClick: () => actionRestore(ctx, row) });
        } else if (isArchive) {
            items.push({ icon: "inbox", label: t("reading.move_to_inbox"), onClick: () => actionMoveToInbox(ctx, row) });
        } else {
            items.push({ icon: "archive", label: t("reading.archive"), onClick: () => actionArchive(ctx, row) });
        }
    }

    items.push({
        icon: "trash",
        label: isTrash ? t("reading.permanent_delete") : t("reading.delete"),
        danger: true,
        onClick: () => actionDelete(ctx, row),
    });

    if (!isSent) {
        items.push({ icon: "mail", label: t("reading.mark_unread"), onClick: () => actionMarkUnread(ctx, row) });
    }

    if (!isDraft && !isSent && !isTrash) {
        items.push({
            icon: "star",
            label: isStarred ? t("reading.unstar") : t("reading.star"),
            onClick: () => actionToggleStar(ctx, row),
        });
    }

    if (!items.length) return;
    buildMenu(x, y, items);
}

export function bindEmailListContextMenu(ctx) {
    const list = document.getElementById("email-list");
    if (!list) return;

    list.addEventListener("contextmenu", (event) => {
        const target = event.target instanceof Element ? event.target : event.target?.parentElement;
        const row = target?.closest?.(".email-item");
        if (!row) return;
        event.preventDefault();
        event.stopPropagation();
        void openMenu(row, event.clientX, event.clientY, ctx);
    });
}