import { getMailboxCounts, getInboxCategories, categorizeInbox, abortCategorizeInbox, getCategorizeProgress, renameInboxCategory } from "../api.js";
import { refreshSmartInboxEnabled, setSmartInboxEnabled } from "../lib/smartInbox.js";
import { mailboxTitle, escapeHtml } from "../lib/format.js";
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
        const chevron = navItem.querySelector("#inbox-expand-btn");
        if (chevron && badge.nextSibling !== chevron) navItem.insertBefore(badge, chevron);
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
            document.querySelectorAll(".sidebar .nav-item").forEach((n) => n.classList.remove("active"));
            item.classList.add("active");
            await onMailboxSelect(mailbox);
        });
    }
}

export async function bindSmartInbox(onMailboxSelect) {
    const list = document.getElementById("smart-categories");
    const toggle = document.getElementById("inbox-expand-btn");
    const organize = document.getElementById("smart-organize-btn");
    const inbox = document.querySelector('[data-mailbox="INBOX"]');
    if (!list || !toggle || !organize || !inbox) return;
    let categories = [];
    let enabled = true;
    let expanded = localStorage.getItem("verdant.smartInboxExpanded") !== "0";

    const setExpanded = (next) => {
        expanded = !!next;
        localStorage.setItem("verdant.smartInboxExpanded", expanded ? "1" : "0");
        render();
    };

    const render = () => {
        list.hidden = !enabled || !expanded || categories.length === 0;
        organize.hidden = enabled || !expanded;
        toggle.hidden = false;
        toggle.innerHTML = icon(expanded ? "chevron-up" : "chevron-down");
        toggle.title = expanded ? t("smart.collapse") : t("smart.expand");
        toggle.setAttribute("aria-label", toggle.title);
        list.innerHTML = categories.map(c => `
          <div class="nav-item smart-category" data-category="${escapeHtml(c.slug)}">
            <span class="smart-category-dot" style="--category-color: ${escapeHtml(c.color || "#6c7065")}"></span>
            <span class="nav-text smart-category-name">${escapeHtml(c.name)}</span>
            <button class="smart-rename" data-rename="${escapeHtml(c.slug)}" title="${escapeHtml(t("smart.rename"))}" aria-label="${escapeHtml(t("smart.rename"))}">${icon("pencil")}</button>
            ${c.unread_count ? `<span class="nav-badge subtle">${c.unread_count}</span>` : ""}
          </div>`).join("");
        list.querySelectorAll(".smart-category").forEach(item => item.addEventListener("click", e => {
            if (e.target.closest("[data-rename]") || item.classList.contains("editing")) return;
            onMailboxSelect(`CATEGORY:${item.dataset.category}`);
        }));
        list.querySelectorAll("[data-rename]").forEach(btn => btn.addEventListener("click", async e => {
            e.stopPropagation();
            const item = btn.closest(".smart-category");
            const category = categories.find(c => c.slug === btn.dataset.rename);
            if (!category || item.classList.contains("editing")) return;
            item.classList.add("editing");
            const name = item.querySelector(".smart-category-name");
            name.innerHTML = `<input class="smart-rename-input" value="${escapeHtml(category.name)}" maxlength="40" aria-label="${escapeHtml(t("smart.rename"))}">`;
            btn.innerHTML = icon("check");
            btn.title = t("smart.confirm_rename");
            const input = item.querySelector("input");
            input.focus();
            input.select();
            const save = async () => {
                const next = input.value.trim();
                if (!next || next === category.name) { render(); return; }
                try {
                    await renameInboxCategory(category.slug, next);
                    categories = await getInboxCategories();
                    render();
                } catch (error) {
                    console.error("Failed to rename smart inbox category", error);
                    render();
                }
            };
            btn.onclick = save;
            input.addEventListener("keydown", event => {
                if (event.key === "Enter") save();
                if (event.key === "Escape") render();
            });
        }));
    };
    const refresh = async () => {
        try {
            enabled = await refreshSmartInboxEnabled();
            categories = enabled ? await getInboxCategories() : [];
            render();
        }
        catch (error) { console.error("Failed to load smart inbox", error); }
    };
    toggle.addEventListener("click", event => {
        event.stopPropagation();
        setExpanded(!expanded);
    });
    organize.addEventListener("click", async () => {
        try {
            await setSmartInboxEnabled(true);
            await refresh();
            openSmartInboxSetup(refresh);
        } catch (error) {
            console.error("Failed to enable smart inbox", error);
        }
    });
    window.addEventListener("smart-inbox-enabled", event => {
        enabled = !!event.detail?.enabled;
        categories = enabled ? categories : [];
        render();
        refresh();
        if (enabled && event.detail?.openOnboarding) {
            setExpanded(true);
            openSmartInboxSetup(refresh);
        }
    });
    window.addEventListener("smart-inbox-request-onboarding", () => {
        enabled = true;
        setExpanded(true);
        openSmartInboxSetup(refresh);
    });
    await refresh();
}

function closeSmartInboxSetup() {
    document.querySelectorAll(".smart-modal-overlay").forEach((overlay) => overlay.remove());
}

function openSmartInboxSetup(refresh) {
    const overlay = document.createElement("div");
    overlay.className = "smart-modal-overlay";
    overlay.innerHTML = `
      <section class="smart-modal" role="dialog" aria-modal="true" aria-labelledby="smart-modal-title">
        <button class="smart-modal-close" aria-label="${escapeHtml(t("reading.close"))}">${icon("x")}</button>
        <div class="smart-modal-step" data-step="intro">
          <div class="smart-modal-icon">${icon("sparkles")}</div>
          <h2 id="smart-modal-title">${escapeHtml(t("smart.title"))}</h2>
          <p>${escapeHtml(t("smart.subtitle"))}</p>
          <div class="smart-demo"><span>${icon("mail")}</span><i></i><span class="smart-demo-card">${escapeHtml(t("smart.demo_work"))}</span><span class="smart-demo-card">${escapeHtml(t("smart.demo_news"))}</span><span class="smart-demo-card">${escapeHtml(t("smart.demo_other"))}</span></div>
          <p class="smart-modal-note">${icon("info-circle")} ${escapeHtml(t("smart.notice"))}</p>
          <div class="smart-modal-actions"><button class="verdant-btn smart-start">${escapeHtml(t("smart.start"))}</button></div>
        </div>
        <div class="smart-modal-step" data-step="progress" hidden>
          <div class="smart-modal-icon is-spinning">${icon("sparkles")}</div>
          <h2>${escapeHtml(t("smart.progress_title"))}</h2>
          <p class="smart-progress-label">${escapeHtml(t("smart.progress_body"))}</p>
          <div class="smart-progress"><div></div></div>
          <div class="smart-progress-percent">0%</div>
          <div class="smart-modal-actions"><button class="smart-abort verdant-btn secondary">${escapeHtml(t("smart.abort"))}</button></div>
        </div>
        <div class="smart-modal-step" data-step="done" hidden>
          <div class="smart-modal-icon smart-done">${icon("check")}</div>
          <h2>${escapeHtml(t("smart.done"))}</h2>
          <p class="smart-done-body"></p>
          <div class="smart-modal-actions"><button class="verdant-btn smart-finish">${escapeHtml(t("smart.finish"))}</button></div>
        </div>
      </section>`;
    document.body.appendChild(overlay);
    requestAnimationFrame(() => overlay.classList.add("open"));
    const modal = overlay.querySelector(".smart-modal");
    const close = () => { overlay.classList.remove("open"); setTimeout(() => overlay.remove(), 180); };
    const setStep = step => overlay.querySelectorAll(".smart-modal-step").forEach(el => { el.hidden = el.dataset.step !== step; });
    overlay.querySelector(".smart-modal-close").onclick = close;
    overlay.addEventListener("click", event => { if (event.target === overlay && !overlay.querySelector('[data-step="progress"]:not([hidden])')) close(); });
    overlay.querySelector(".smart-finish").onclick = async () => { await refresh(); close(); };
    overlay.querySelector(".smart-start").onclick = async () => {
        // Persist activation before analysis, including onboarding opened by
        // another caller rather than the sidebar button.
        await setSmartInboxEnabled(true);
        await refresh();
        setStep("progress");
        overlay.querySelector(".smart-modal-close").hidden = true;
        const bar = overlay.querySelector(".smart-progress > div");
        const percent = overlay.querySelector(".smart-progress-percent");
        let progressTimer = null;
        const poll = async () => {
            try {
                const p = await getCategorizeProgress();
                const done = Number(p?.processed || 0), total = Number(p?.total || 0);
                const value = total ? Math.min(100, Math.round(done * 100 / total)) : 0;
                bar.style.width = `${value}%`;
                percent.textContent = total ? `${done} / ${total} (${value}%)` : "0%";
                overlay.querySelector(".smart-progress-label").textContent = t("smart.progress", { done, total });
            } catch {}
        };
        progressTimer = setInterval(poll, 180);
        poll();
        try {
            const result = await categorizeInbox();
            clearInterval(progressTimer);
            bar.style.width = "100%";
            percent.textContent = "100%";
            overlay.querySelector(".smart-done-body").textContent = t("smart.done_body", { n: result?.assigned || 0 });
            setStep("done");
        } catch (error) {
            clearInterval(progressTimer);
            console.error("Smart inbox analysis failed", error);
            close();
        }
    };
    overlay.querySelector(".smart-abort").onclick = async () => {
        await abortCategorizeInbox();
        overlay.querySelector(".smart-progress-label").textContent = t("smart.abort");
        close();
    };
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
    const maxWidth = () => Math.max(minWidth, Math.min(window.innerWidth * 0.68, 760));

    const applyWidth = (width) => {
        const next = Math.max(minWidth, Math.min(Math.round(width), maxWidth()));
        pane.style.width = `${next}px`;
        pane.style.minWidth = `${next}px`;
        pane.style.flex = `0 0 ${next}px`;
        try {
            localStorage.setItem(STORAGE_KEY, String(next));
        } catch (error) {
            console.error("Failed to persist list pane width", error);
        }
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
