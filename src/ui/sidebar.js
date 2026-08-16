import { getMailboxCounts } from "../api.js";
import { mailboxTitle } from "../lib/format.js";
import { t } from "../lib/i18n.js";
import { icon } from "./icons.js";

const SIDEBAR_COLLAPSED_KEY = "verdant.sidebarCollapsed";
let pendingCollapsed = null;
let hoverOutDone = null;

function setCollapseBtn(collapsed) {
    const btn = document.getElementById("sidebar-collapse-btn");
    if (!btn) return;
    btn.innerHTML = collapsed ? icon("sidebar-expand") : icon("sidebar-collapse");
    const label = collapsed ? t("sidebar.expand") : t("sidebar.collapse");
    btn.title = label;
    btn.setAttribute("aria-label", label);
}

function persistSidebarCollapsed(collapsed) {
    try { localStorage.setItem(SIDEBAR_COLLAPSED_KEY, collapsed ? "1" : "0"); } catch {}
    import("@tauri-apps/api/core").then(({ invoke }) => {
        invoke("update_app_config", { config: { sidebar_collapsed: collapsed } })
            .catch(err => console.error("Failed to persist sidebar state:", err));
    });
}

function cancelHoverOut() {
    if (hoverOutDone) { hoverOutDone(); hoverOutDone = null; }
}

export function applySidebarCollapsed(collapsed) {
    const sidebar = document.querySelector(".sidebar");
    if (!sidebar) {
        pendingCollapsed = collapsed;
        return;
    }
    pendingCollapsed = null;
    setCollapseBtn(collapsed);
    persistSidebarCollapsed(collapsed);
    document.body.classList.toggle("sidebar-collapsed", collapsed);
    sidebar.classList.remove("sidebar-float", "sidebar-float-expanded", "sidebar-float-exit");
}

function bindSidebarHover() {
    const sidebar = document.querySelector(".sidebar");
    if (!sidebar) return;

    let hoverOutTimer = null;

    sidebar.addEventListener("mouseenter", () => {
        if (!document.body.classList.contains("sidebar-collapsed")) return;
        if (hoverOutTimer) { clearTimeout(hoverOutTimer); hoverOutTimer = null; }
        if (hoverOutDone) { hoverOutDone(); hoverOutDone = null; }
        if (sidebar.classList.contains("sidebar-float")) {
            sidebar.classList.add("sidebar-float-expanded");
            return;
        }
        sidebar.classList.add("sidebar-float");
        void sidebar.offsetWidth;
        sidebar.classList.add("sidebar-float-expanded");
    });

    sidebar.addEventListener("mouseleave", () => {
        if (!document.body.classList.contains("sidebar-collapsed")) return;
        if (!sidebar.classList.contains("sidebar-float")) return;
        hoverOutTimer = setTimeout(() => {
            hoverOutTimer = null;
            if (!sidebar.classList.contains("sidebar-float-expanded")) return;
            sidebar.classList.remove("sidebar-float-expanded");
            const done = (e) => {
                if (e.target !== sidebar || e.propertyName !== "transform") return;
                sidebar.removeEventListener("transitionend", done);
                hoverOutDone = null;
                sidebar.classList.add("sidebar-float-exit");
                void sidebar.offsetWidth;
                sidebar.classList.remove("sidebar-float");
                void sidebar.offsetWidth;
                sidebar.classList.remove("sidebar-float-exit");
            };
            hoverOutDone = () => sidebar.removeEventListener("transitionend", done);
            sidebar.addEventListener("transitionend", done);
        }, 300);
    });
}

export function bindSidebarCollapse() {
    const btn = document.getElementById("sidebar-collapse-btn");
    if (!btn) return;

    if (pendingCollapsed !== null) {
        applySidebarCollapsed(pendingCollapsed);
    } else {
        try { applySidebarCollapsed(localStorage.getItem(SIDEBAR_COLLAPSED_KEY) === "1"); } catch {}
    }

    bindSidebarHover();

    btn.addEventListener("click", () => {
        const sidebar = document.querySelector(".sidebar");
        const collapsed = document.body.classList.contains("sidebar-collapsed");
        if (collapsed) {
            persistSidebarCollapsed(false);
            setCollapseBtn(false);
            cancelHoverOut();
            document.body.classList.remove("sidebar-collapsed");
            sidebar.classList.remove("sidebar-float", "sidebar-float-expanded", "sidebar-float-exit");
        } else {
            persistSidebarCollapsed(true);
            setCollapseBtn(true);
            document.body.classList.add("sidebar-collapsed");
        }
    });
}

export function setAppHeaderSubtitle(label) {
    const subtitle = document.querySelector(".app-subtitle");
    if (!subtitle) return;
    subtitle.textContent = `- ${(label || t("sidebar.mailbox_fallback")).trim()}`;
}

export function refreshAppHeaderSubtitle(currentMailbox, isComposeOpen, isSettingsOpen) {
    if (isComposeOpen()) {
        setAppHeaderSubtitle(t("sidebar.compose_title"));
        return;
    }
    const overlay = document.getElementById("verdant-overlay");
    if (overlay && isSettingsOpen()) {
        const heading = overlay.querySelector(".verdant-head h2")?.textContent?.trim();
        if (heading) {
            setAppHeaderSubtitle(heading);
            return;
        }
    }
    setAppHeaderSubtitle(mailboxTitle(currentMailbox));
}

export function setListTitle(mailbox, count) {
    const title = document.querySelector(".list-title");
    const countEl = document.querySelector(".list-count");
    if (title) title.textContent = mailboxTitle(mailbox);
    if (countEl) countEl.textContent = t("list.count", { n: count });
}

function setBadge(navItem, value, mailbox) {
    if (!navItem) return;
    let badge = navItem.querySelector(".nav-badge");
    if (value <= 0) { badge?.remove(); return; }
    if (!badge) {
        badge = document.createElement("span");
        badge.className = "nav-badge";
        navItem.appendChild(badge);
    }
    badge.textContent = String(value);
    
    if (mailbox === "INBOX") {
        badge.classList.remove("subtle");
    } else {
        badge.classList.add("subtle");
    }
}

export async function refreshCounts() {
    const counts = await getMailboxCounts();
    const items = Array.from(document.querySelectorAll(".sidebar .nav-item"));
    const find = (mb) => items.find((n) => n.dataset.mailbox === mb);
    const mCounts = {
        INBOX: counts.inbox_unread,
        STARRED: counts.starred_total,
        SENT: counts.sent_total,
        DRAFT: counts.drafts_total,
        ARCHIVE: counts.archive_total,
        TRASH: counts.trash_total,
    };
    for (const mailboxId in mCounts) {
        setBadge(find(mailboxId), mCounts[mailboxId], mailboxId);
    }
}

export function bindMailboxNav(onMailboxSelect) {
    const items = Array.from(document.querySelectorAll(".sidebar .nav-item"));
    for (const item of items) {
        const mailbox = item.dataset.mailbox;
        if (!mailbox) continue;
        item.addEventListener("click", async () => {
            items.forEach((n) => n.classList.remove("active"));
            item.classList.add("active");
            await onMailboxSelect(mailbox);
        });
    }
}

export function setUserProfile(profile) {
    const avatar = document.getElementById("user-avatar");
    const name = document.getElementById("user-name");
    const email = document.getElementById("user-email");
    if (avatar) avatar.textContent = profile.initials;
    if (name) name.textContent = profile.name;
    if (email) email.textContent = profile.email;
}


export function bindUserRow(onAccountPopover) {
    const row = document.getElementById("user-row");
    if (row) row.onclick = onAccountPopover;
}

export function bindPaneResizer() {
    const pane = document.querySelector(".email-list-pane");
    const resizer = document.getElementById("pane-resizer");
    if (!pane || !resizer) return;

    const STORAGE_KEY = "verdant.listPaneWidth";
    const minWidth = 260;
    const maxWidth = () => Math.min(window.innerWidth * 0.68, 760);

    const applyWidth = (width) => {
        const next = Math.max(minWidth, Math.min(Math.round(width), maxWidth()));
        pane.style.width = `${next}px`;
        pane.style.minWidth = `${next}px`;
        pane.style.flex = `0 0 ${next}px`;
        localStorage.setItem(STORAGE_KEY, String(next));
    };

    const saved = Number(localStorage.getItem(STORAGE_KEY));
    if (Number.isFinite(saved) && saved > 0) applyWidth(saved);

    const onPointerDown = (event) => {
        if (window.innerWidth <= 980) return;
        event.preventDefault();
        document.body.classList.add("resizing");
        resizer.setPointerCapture?.(event.pointerId);

        const startX = event.clientX;
        const startWidth = pane.getBoundingClientRect().width;

        const onMove = (moveEvent) => {
            moveEvent.preventDefault();
            applyWidth(startWidth + (moveEvent.clientX - startX));
        };

        const onUp = () => {
            document.body.classList.remove("resizing");
            window.removeEventListener("pointermove", onMove);
            window.removeEventListener("pointerup", onUp);
            window.removeEventListener("pointercancel", onUp);
        };

        window.addEventListener("pointermove", onMove);
        window.addEventListener("pointerup", onUp);
        window.addEventListener("pointercancel", onUp);
    };

    resizer.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("resize", () => {
        const current = pane.getBoundingClientRect().width;
        if (current > maxWidth()) applyWidth(current);
    });
}

export function bindAppHeaderControls(isComposeOpen, isSettingsOpen, currentMailboxFn) {
    const minBtn = document.getElementById("app-min-btn");
    const maxBtn = document.getElementById("app-max-btn");
    const closeBtn = document.getElementById("app-close-btn");
    const header = document.querySelector(".app-header");
    const controls = document.querySelector(".app-header-controls");

    if (!minBtn || !maxBtn || !closeBtn || !header) return;

    try {
        import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
            const appWindow = getCurrentWindow();
            controls?.removeAttribute("data-tauri-drag-region");

            header.addEventListener("pointerdown", async (event) => {
                if (event.button !== 0) return;
                const target = event.target;
                if (!(target instanceof Element)) return;
                if (target.closest(".app-header-controls")) return;
                if (event.detail > 1) return;
                try { await appWindow.startDragging(); } catch {}
            });

            minBtn.addEventListener("click", async () => { try { await appWindow.minimize(); } catch {} });
            maxBtn.addEventListener("click", async () => { try { await appWindow.toggleMaximize(); } catch {} });
            closeBtn.addEventListener("click", async () => { try { await appWindow.close(); } catch {} });

            header.addEventListener("dblclick", async (event) => {
                const target = event.target;
                if (target instanceof Element && target.closest(".app-header-controls")) return;
                try { await appWindow.toggleMaximize(); } catch {}
            });
        }).catch(() => {
            minBtn.style.display = "none";
            maxBtn.style.display = "none";
            closeBtn.style.display = "none";
        });
    } catch {
        minBtn.style.display = "none";
        maxBtn.style.display = "none";
        closeBtn.style.display = "none";
    }

    window.addEventListener("verdant-compose-opened", () =>
        refreshAppHeaderSubtitle(currentMailboxFn(), isComposeOpen, isSettingsOpen)
    );
    window.addEventListener("verdant-compose-closed", () =>
        refreshAppHeaderSubtitle(currentMailboxFn(), isComposeOpen, isSettingsOpen)
    );
}
