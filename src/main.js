(() => {
  const APP_PREFS_KEY = "verdant.appPrefs";
  try {
    const appPrefsRaw = localStorage.getItem(APP_PREFS_KEY);
    const appPrefs = appPrefsRaw ? JSON.parse(appPrefsRaw) : {};
    if (appPrefs.useDarkMode === true) {
      document.documentElement.classList.add("dark");
    }
  } catch (err) {
    console.error("Theme preference check failed:", err);
  }
})();

import { authStatus, getUserProfile, getEmails, syncMailboxPage, syncImapMailboxPage, getStartupFlags, hideMainWindow } from "./api.js";
import { setEmailReadStatus } from "./api.js";
import { openExternalUrl } from "./api.js";
import { ingestContactsFromEmails, ensureContactsLoaded } from "./lib/contacts.js";
import { getHotkeys, normalizeCombo, eventCombo, canRunHotkey } from "./lib/hotkeys.js";
import { showToast } from "./lib/toast.js";
import { escapeHtml, sanitizeUnicodeNoise, formatListDate, mailboxTitle } from "./lib/format.js";
import { syncMailboxInBackground, startPeriodicSync, mailboxNextPageToken, knownInboxIds, setKnownInboxIds } from "./lib/sync.js";
import "./ui/styles/dynamic.css";
import "./ui/styles/shell.css";
import "./ui/styles/onboarding.css";
import "./ui/styles/accounts.css";
import "./ui/styles/updates.css";
import "./ui/styles/whatsnew.css";
import "./ui/styles/contextmenu.css";
import { renderShell } from "./ui/shell.js";
import { showOnboarding } from "./ui/onboarding.js";
import {
    bindMailboxNav, bindPaneResizer, bindAppHeaderControls,
    refreshCounts, setUserProfile, bindUserRow, setListTitle, refreshAppHeaderSubtitle,
    bindSidebarCollapse,
} from "./ui/sidebar.js";
import {
    renderReadingPane, bindReadingActions, setReadingPaneHidden,
    applySenderAvatar, hasEmailAttachments, updateTopActionStates,
} from "./ui/reading.js";
import {
    isComposeOpen, openCompose, closeCompose, openComposeForDraft,
    bindComposeRecipientInputs, bindComposeFormatting, bindComposeAttachments,
    bindComposeWindowControls, bindComposeSend, bindComposeDraftSave, bindComposeClear,
} from "./ui/compose.js";
import {
    openSettingsModal, isSettingsOpen, closeOverlay,
    updatePrefs, hydratePrefsFromBackend, runAutomaticUpdateFlow,
} from "./ui/settings.js";
import { openAccountPopover, closeAccountPopover } from "./ui/accounts.js";
import { openWhatsNewModal } from "./ui/whatsnew.js";
import { bindEmailListContextMenu } from "./ui/contextmenu.js";
import { appPrefs } from "./ui/settings.js";
import { checkForUpdates, downloadLatestUpdate, switchAccount, listAccounts } from "./api.js";
import { getInboxThreads } from "./api.js";
import {
    renderThreadList,
    getSelectedThreadId, getSelectedThreadLatestMessage, clearSelectedThread, getThreadById,
} from "./ui/thread.js";
import { t, initLang } from "./lib/i18n.js";
import { icon } from "./ui/icons.js";

const PERIODIC_UPDATE_CHECK_INTERVAL_MS = 30 * 60 * 1000;

let profileRetryTimeout = null;

async function retryUserProfile(attempt = 0) {
    if (profileRetryTimeout) clearTimeout(profileRetryTimeout);
    if (attempt >= 8) return;

    const delay = Math.min(1000 * Math.pow(2, attempt), 60000);
    profileRetryTimeout = setTimeout(async () => {
        try {
            const profile = await getUserProfile();
            if (profile.degraded) {
                retryUserProfile(attempt + 1);
            } else {
                setUserProfile(profile);
            }
        } catch {
            retryUserProfile(attempt + 1);
        }
    }, delay);
}


let currentMailbox = "INBOX";
let currentEmails = [];
let selectedEmail = null;
let activeFilter = "Important";
let searchQuery = "";
let isDeepSearchActive = false;
let isFetchingMore = false;
let isSyncing = false;

const mailboxCache = new Map();
let inboxThreadsCache = null;
let lastAnimatedRenderAt = 0;
const ANIMATION_WINDOW_MS = 1800;

function showBootScreen() {
    if (document.getElementById("boot-screen")) return;
    const boot = document.createElement("div");
    boot.id = "boot-screen";
    boot.className = "boot-screen";
    boot.innerHTML = '<div class="boot-spinner"></div>';
    document.body.appendChild(boot);
}

function hideBootScreen() {
    document.getElementById("boot-screen")?.remove();
}

function resetMailboxCaches() {
    mailboxCache.clear();
    inboxThreadsCache = null;
    currentEmails = [];
    mailboxNextPageToken.clear();
}

function showListLoading(visible = true) {
    const list = document.getElementById("email-list");
    if (!list) return;
    if (visible) {
        list.innerHTML = '<div class="list-loading"><div class="list-spinner"></div></div>';
    } else if (list.querySelector(".list-loading")) {
        list.innerHTML = "";
    }
}

async function loadInboxThreads(animate = false) {
    if (inboxThreadsCache) {
        renderThreadList(inboxThreadsCache, activeFilter, searchQuery, animate);
        return;
    }
    showListLoading();
    try {
        const threads = await getInboxThreads();
        inboxThreadsCache = threads;
        showListLoading(false);
        renderThreadList(threads, activeFilter, searchQuery, animate);
    } catch (e) {
        showListLoading(false);
        console.error("Failed to load inbox threads", e);
    }
}



async function runStartupUpdateCheck() {
    try {
        const channel = updatePrefs?.channel || "stable";
        const info = await checkForUpdates(channel);
        if (!info?.updateAvailable) return;

        // Nightly dedup: don't re-notify for same build
        if (channel === "nightly") {
            const lastUrl = localStorage.getItem("verdant.lastNightlyUrl");
            if (info.downloadUrl === lastUrl) return;
            localStorage.setItem("verdant.lastNightlyUrl", info.downloadUrl);
        }

        const toast = document.createElement("div");
        toast.className = "update-toast";
        toast.innerHTML = `
            <div class="update-toast-header">
                <div>
                    <div class="update-toast-title">${t("update.title", { version: escapeHtml(info.latestVersion) })}</div>
                    <div class="update-toast-sub">${escapeHtml(info.releaseName || "")}</div>
                </div>
                <button class="update-toast-close" id="update-toast-close">×</button>
            </div>
            <div class="update-progress-wrap" id="update-progress-wrap">
                <div class="update-progress-label" id="update-progress-label">${t("update.downloading")}</div>
                <div class="update-progress-track"><div class="update-progress-bar indeterminate" id="update-progress-bar"></div></div>
            </div>
            <div class="update-toast-actions" id="update-toast-actions">
                <button class="update-toast-btn" id="update-toast-dismiss">${t("update.later")}</button>
                <button class="update-toast-btn primary" id="update-toast-download">${t("update.download")}</button>
            </div>
        `;
        document.body.appendChild(toast);
        requestAnimationFrame(() => toast.classList.add("open"));

        const close = () => { toast.classList.remove("open"); setTimeout(() => toast.remove(), 350); };
        toast.querySelector("#update-toast-close").onclick = close;
        toast.querySelector("#update-toast-dismiss").onclick = close;

        toast.querySelector("#update-toast-download").onclick = async () => {
            toast.querySelector("#update-toast-actions").style.display = "none";
            toast.querySelector("#update-toast-close").style.display = "none";
            const progressWrap = toast.querySelector("#update-progress-wrap");
            const progressLabel = toast.querySelector("#update-progress-label");
            progressWrap.classList.add("visible");
            try {
                progressLabel.textContent = t("update.downloading");
                const result = await downloadLatestUpdate(channel);
                progressLabel.textContent = t("update.installing");
                const { invoke } = await import("@tauri-apps/api/core");
                await invoke("install_and_relaunch", { filePath: result.filePath });
                progressLabel.textContent = t("update.restarting");
                await new Promise((r) => setTimeout(r, 800));
                const { exit } = await import("@tauri-apps/plugin-process");
                await exit(0);
            } catch (err) {
                progressLabel.textContent = t("update.failed", { error: String(err) });
                toast.querySelector("#update-progress-bar").classList.remove("indeterminate");
                toast.querySelector("#update-progress-bar").style.background = "#c08d8d";
                setTimeout(() => {
                    toast.querySelector("#update-toast-actions").style.display = "flex";
                    toast.querySelector("#update-toast-close").style.display = "flex";
                    toast.querySelector("#update-toast-dismiss").textContent = t("reading.close");
                    toast.querySelector("#update-toast-download").style.display = "none";
                }, 1800);
            }
        };
    } catch {}
}

function startPeriodicUpdateCheck() {

    setInterval(async () => {
        if (updatePrefs.autoCheck) {
            try {
                await runAutomaticUpdateFlow();
            } catch (err) {
                console.error("Periodic update check failed:", err);
            }
        }
    }, PERIODIC_UPDATE_CHECK_INTERVAL_MS);
}


async function checkAndShowWhatsNewModal() {
    try {
        const { Store } = await import("@tauri-apps/plugin-store");
        const store = await Store.load("verdant.json");
        
        const { invoke } = await import("@tauri-apps/api/core");
        const updateInfo = await invoke("check_for_updates");
        const currentVersion = updateInfo.currentVersion;
        
        const lastSeenVersion = await store.get("lastSeenVersion");
        
        if (lastSeenVersion !== currentVersion) {
            await openWhatsNewModal(currentVersion);
            await store.set("lastSeenVersion", currentVersion);
            await store.save();
        }
    } catch (err) {
        console.error("Failed to check for What's New:", err);
    }
}


function isImportant(email) {
    const labels = (email.labels || "").split(",").map(l => l.trim());
    return !labels.some(l => l === "SPAM" || l === "TRASH" || l === "CATEGORY_PROMOTIONS");
}

function emailMatchesFilter(email) {
    if (activeFilter === "Important") {
        if (currentMailbox !== "TRASH" && currentMailbox !== "SPAM") {
            if (!isImportant(email)) return false;
        }
    }
    if (activeFilter === "Unread" && email.is_read) return false;
    if (activeFilter === "Attachments" && !hasEmailAttachments(email)) return false;
    if (searchQuery) {
        const hay = `${email.subject || ""} ${email.sender || ""} ${email.snippet || ""}`.toLowerCase();
        if (!hay.includes(searchQuery.toLowerCase())) return false;
    }
    return true;
}

function visibleEmails() {
    return (currentEmails || []).filter(emailMatchesFilter);
}

function renderEmailList(animate = false) {
    const list = document.getElementById("email-list");
    if (!list) return;
    list.innerHTML = "";
    list.classList.toggle("suppress-anim", !animate);

    const emails = visibleEmails();
    setListTitle(currentMailbox, emails.length);
    const selectedId = selectedEmail?.id || null;
    let selectedRow = null, selectedRowEmail = null;

    for (let i = 0; i < emails.length; i++) {
        const email = emails[i];
        const row = document.createElement("div");
        row.className = `email-item ${email.is_read ? "" : "unread"}`.trim();
        row.dataset.emailId = email.id;
        if (animate) row.style.animationDelay = `${Math.min(i * 40, 1200)}ms`;
        row.innerHTML = `
            ${email.is_read ? "" : '<div class="unread-dot"></div>'}
            ${email.starred ? `<span class="star-badge">${icon("star-filled", 18)}</span>` : ""}
            <div class="email-item-main">
                <div class="sender-avatar"></div>
                <div class="email-item-inner">
                    <div class="email-top">
                        <span class="email-sender">${escapeHtml(sanitizeUnicodeNoise(email.sender || t("app.unknown_sender")))}</span>
                        <span class="email-time">${escapeHtml(formatListDate(email.date))}</span>
                    </div>
                    <div class="email-subject">${escapeHtml(sanitizeUnicodeNoise(email.subject || t("app.no_subject")))}</div>
                    <div class="email-preview">${escapeHtml(sanitizeUnicodeNoise(email.snippet || ""))}</div>
                </div>
            </div>
        `;
        applySenderAvatar(row.querySelector(".sender-avatar"), email.sender || "", email.mailbox || "");
        row.addEventListener("click", () => selectEmail(email, row));

        if (selectedId && email.id === selectedId) {
            row.classList.add("active");
            selectedRow = row;
            selectedRowEmail = email;
        }
        list.appendChild(row);
    }

    if (selectedRow && selectedRowEmail) {
        selectedEmail = selectedRowEmail;
        renderReadingPane(selectedRowEmail, currentMailbox);
    }
}

async function selectEmail(email, row) {
    setReadingPaneHidden(false);
    selectedEmail = email;
    document.querySelectorAll(".email-item").forEach((el) => el.classList.remove("active"));
    row.classList.add("active");
    row.classList.remove("unread");
    row.querySelector(".unread-dot")?.remove();
    renderReadingPane(email, currentMailbox);
    await markSelectedAsReadIfNeeded();
}

async function markSelectedAsReadIfNeeded() {
    if (!selectedEmail || selectedEmail.is_read) return;
    selectedEmail.is_read = true;
    await setEmailReadStatus(selectedEmail.id, true);
    await refreshCounts();
}

async function loadLocalMailbox(mailbox, animate = false) {
    const mailboxChanged = currentMailbox !== mailbox;
    if (mailboxChanged) {
        selectedEmail = null;
        clearSelectedThread();
        isDeepSearchActive = false;
        setReadingPaneHidden(true);
    }
    currentMailbox = mailbox;

    if (mailbox === "INBOX") {
        setListTitle(mailbox, 0);
        if (inboxThreadsCache) {
            renderThreadList(inboxThreadsCache, activeFilter, searchQuery, animate);
        } else {
            showListLoading();
            try {
                const threads = await getInboxThreads();
                inboxThreadsCache = threads;
                if (currentMailbox !== mailbox) return;
                renderThreadList(threads, activeFilter, searchQuery, animate);
            } catch (e) {
                console.error("Failed to load inbox threads", e);
            } finally {
                if (currentMailbox === mailbox) showListLoading(false);
            }
        }
    } else {
        const cached = mailboxCache.get(mailbox);
        if (cached) {
            currentEmails = cached;
            renderEmailList(animate);
        } else {
            showListLoading();
            try {
                const fetched = await getEmails(mailbox);
                mailboxCache.set(mailbox, fetched);
                if (currentMailbox !== mailbox) return;
                currentEmails = fetched;
                ingestContactsFromEmails(fetched);
                renderEmailList(animate);
            } catch (e) {
                console.error(`Failed to load mailbox ${mailbox}`, e);
            } finally {
                if (currentMailbox === mailbox) showListLoading(false);
            }
        }
    }

    refreshAppHeaderSubtitle(currentMailbox, isComposeOpen, isSettingsOpen);
    refreshCounts().catch(console.error);
}

async function openMailbox(mailbox, animate = false, forceSync = true) {
    const loadPromise = loadLocalMailbox(mailbox, animate).catch(console.error);
    if (animate) lastAnimatedRenderAt = Date.now();
    const syncPromise = forceSync
        ? syncMailboxInBackground(mailbox, true, onSynced).catch(console.error)
        : Promise.resolve();
    await loadPromise;
    await syncPromise;
    if (currentMailbox === mailbox) {
        const wait = Math.max(0, ANIMATION_WINDOW_MS - (Date.now() - lastAnimatedRenderAt));
        if (wait > 0) {
            setTimeout(() => {
                if (currentMailbox === mailbox) loadLocalMailbox(mailbox, false).catch(console.error);
            }, wait);
        } else {
            loadLocalMailbox(mailbox, false).catch(console.error);
        }
    }
}

function onSynced(mailbox, latestEmails) {
  if (Date.now() - lastAnimatedRenderAt < ANIMATION_WINDOW_MS) return;
  if (currentMailbox === mailbox) {
    if (mailbox === "INBOX") {
      getInboxThreads().then(threads => {
        inboxThreadsCache = threads;
        renderThreadList(threads, activeFilter, searchQuery, false);
        refreshCounts().catch(console.error);
      }).catch(console.error);
    } else {
      currentEmails = latestEmails || currentEmails;
      mailboxCache.set(mailbox, currentEmails);
      renderEmailList(false);
      refreshCounts().catch(console.error);
    }
  }
}

async function refreshFromSyncedEvent() {
  if (Date.now() - lastAnimatedRenderAt < ANIMATION_WINDOW_MS) return;
  console.log("Backend synced emails, refreshing UI...");
  try {
    if (currentMailbox === "INBOX") {
      const threads = await getInboxThreads();
      inboxThreadsCache = threads;
      renderThreadList(threads, activeFilter, searchQuery, false);
    } else {
      await loadLocalMailbox(currentMailbox, false);
    }
    refreshCounts().catch(console.error);
  } catch (err) {
    console.error("Failed to refresh after backend sync", err);
  }
}

async function dustOutRows(rows) {
    const layer = document.createElement("div");
    layer.className = "dust-layer";
    document.body.appendChild(layer);

    const particleAnims = [];
    rows.forEach((row) => {
        const rect = row.getBoundingClientRect();
        row.classList.add("dust-out");
        const baseColor = getComputedStyle(row).color || "rgb(90,94,86)";
        const count = 30;

        for (let i = 0; i < count; i++) {
            const p = document.createElement("div");
            p.className = "dust-particle";
            const size = 1.5 + Math.random() * 3.5;
            const x = rect.left + Math.random() * rect.width;
            const y = rect.top + Math.random() * rect.height;
            p.style.left = `${x}px`;
            p.style.top = `${y}px`;
            p.style.width = `${size}px`;
            p.style.height = `${size}px`;
            const tint = i % 4;
            p.style.background = tint === 0 ? "rgba(255,255,255,0.95)"
                : tint === 1 ? "rgba(255,214,120,0.9)"
                : tint === 2 ? "rgba(175,225,255,0.9)"
                : baseColor;
            layer.appendChild(p);

            const dx = (Math.random() - 0.5) * 110;
            const dy = -15 - Math.random() * 80;
            const rot = (Math.random() - 0.5) * 200;
            const scale = 0.05 + Math.random() * 0.35;
            const anim = p.animate(
                [
                    { transform: "translate(0,0) scale(1)", opacity: 1 },
                    { transform: `translate(${dx}px, ${dy}px) rotate(${rot}deg) scale(${scale})`, opacity: 0 },
                ],
                { duration: 550 + Math.random() * 400, easing: "cubic-bezier(0.15, 0.6, 0.35, 1)", fill: "forwards" }
            );
            particleAnims.push(anim);
        }
    });

    await Promise.all(rows.map((row) => new Promise((resolve) => {
        setTimeout(() => {
            const cs = getComputedStyle(row);
            const h = row.offsetHeight;
            const pt = parseFloat(cs.paddingTop) || 0;
            const pb = parseFloat(cs.paddingBottom) || 0;
            const mt = parseFloat(cs.marginTop) || 0;
            const mb = parseFloat(cs.marginBottom) || 0;
            const collapse = row.animate(
                [
                    { height: `${h}px`, paddingTop: `${pt}px`, paddingBottom: `${pb}px`, marginTop: `${mt}px`, marginBottom: `${mb}px`, opacity: 0 },
                    { height: "0px", paddingTop: "0px", paddingBottom: "0px", marginTop: "0px", marginBottom: "0px", opacity: 0 },
                ],
                { duration: 420, easing: "cubic-bezier(0.4, 0, 0.2, 1)", fill: "forwards" }
            );
            collapse.onfinish = () => { row.remove(); resolve(); };
        }, 130);
    })));

    await Promise.all(particleAnims.map((a) => new Promise((r) => { a.onfinish = r; })));
    layer.remove();
}

function patchInboxThreadCache(email) {
    if (!inboxThreadsCache || !email?.thread_id) return;
    if (inboxThreadsCache.some((t) => t.thread_id === email.thread_id)) return;
    const thread = {
        thread_id: email.thread_id,
        subject: email.subject || t("app.no_subject"),
        participants: email.sender || t("app.unknown_sender"),
        snippet: email.snippet || "",
        latest_ts: email.internal_ts || 0,
        latest_date: email.date || "",
        message_count: 1,
        unread_count: email.is_read ? 0 : 1,
        is_read: !!email.is_read,
        starred: !!email.starred,
        has_attachments: !!(email.has_attachments) || (email.attachments_json && email.attachments_json !== "[]"),
        labels: email.labels || "",
    };
    const idx = inboxThreadsCache.findIndex((t) => (t.latest_ts || 0) < thread.latest_ts);
    if (idx === -1) inboxThreadsCache.push(thread);
    else inboxThreadsCache.splice(idx, 0, thread);
}

async function refreshAfterAction(removedIds = [], movedTo = null, movedEmail = null, removedThreadId = null) {
    const list = document.getElementById("email-list");
    if (removedIds.length) {
        for (const m of Array.from(mailboxCache.keys())) {
            if (m !== currentMailbox) mailboxCache.delete(m);
        }
        if (currentMailbox !== "INBOX") {
            if (movedTo === "INBOX" && movedEmail?.thread_id) {
                patchInboxThreadCache(movedEmail);
            } else {
                inboxThreadsCache = null;
            }
        }
    }
    if (removedIds.length && list && !isDeepSearchActive) {
        const removed = new Set(removedIds);
        const activeRow = list.querySelector(".email-item.active");
        let rows = [];
        if (currentMailbox === "INBOX") {
            if (removedThreadId) {
                rows = Array.from(list.querySelectorAll(".email-item[data-thread-id]"))
                    .filter((r) => r.dataset.threadId === removedThreadId);
            } else if (activeRow) {
                rows = [activeRow];
            } else {
                const tid = getSelectedThreadId();
                if (tid) {
                    rows = Array.from(list.querySelectorAll(".email-item[data-thread-id]"))
                        .filter((r) => r.dataset.threadId === tid);
                }
            }
        } else {
            rows = Array.from(list.querySelectorAll(".email-item[data-email-id]"))
                .filter((r) => removed.has(r.dataset.emailId));
            if (!rows.length && activeRow) rows = [activeRow];
        }
        if (rows.length) {
            if (currentMailbox === "INBOX") {
                const tid = removedThreadId || rows[0]?.dataset.threadId;
                if (tid && inboxThreadsCache) {
                    inboxThreadsCache = inboxThreadsCache.filter((t) => t.thread_id !== tid);
                }
            } else {
                currentEmails = (currentEmails || []).filter((e) => !removed.has(e.id));
                mailboxCache.set(currentMailbox, currentEmails);
            }
            const countEl = document.querySelector(".list-count");
            if (countEl) {
                const m = countEl.textContent.match(/\d+/);
                if (m) countEl.textContent = t("list.count", { n: Math.max(0, Number(m[0]) - rows.length) });
            }
            await dustOutRows(rows);
            refreshCounts().catch(console.error);
            syncMailboxInBackground(currentMailbox, false, () => refreshCounts().catch(console.error)).catch(() => {});
            return;
        }
    }
    if (removedIds.length && !isDeepSearchActive) {
        if (currentMailbox === "INBOX") {
            const tid = removedThreadId || getSelectedThreadId();
            if (tid && inboxThreadsCache) {
                inboxThreadsCache = inboxThreadsCache.filter((t) => t.thread_id !== tid);
                renderThreadList(inboxThreadsCache, activeFilter, searchQuery, false);
                refreshCounts().catch(console.error);
                return;
            }
        } else {
            const removed = new Set(removedIds);
            currentEmails = (currentEmails || []).filter((e) => !removed.has(e.id));
            mailboxCache.set(currentMailbox, currentEmails);
            renderEmailList(false);
            refreshCounts().catch(console.error);
            return;
        }
    }
    await loadLocalMailbox(currentMailbox, false);
    syncMailboxInBackground(currentMailbox, false, onSynced).catch(() => {});
}



async function switchActiveAccount(accountId) {
    try {
        await switchAccount(accountId);
    } catch {}
    resetMailboxCaches();
    try {
        const profile = await getUserProfile();
        setUserProfile(profile);
    } catch {}
    await openMailbox("INBOX", true);
    await refreshCounts();
}

async function handleAccountSwitch(accountId) {
    if (accountId !== null) {
        await switchActiveAccount(accountId);
    } else {
        await openMailbox("INBOX", true);
        await refreshCounts();
    }
}

async function handleAfterAddAccount(acc) {
    
    await handleAccountSwitch(acc?.id || null);
}



function bindInfiniteScroll() {
    const list = document.getElementById("email-list");
    if (!list) return;
    list.addEventListener("scroll", () => {
        const remaining = list.scrollHeight - list.scrollTop - list.clientHeight;
        if (remaining < 80) fetchMoreCurrentMailbox().catch(console.error);
    });
}

function setListFetchIndicator(text = "") {
    const pane = document.querySelector(".email-list-pane");
    if (!pane) return;
    pane.querySelector(".list-fetch-indicator")?.remove();
    if (!text) return;
    const el = document.createElement("div");
    el.className = "list-fetch-indicator";
    el.textContent = text;
    pane.appendChild(el);
}

async function fetchMoreCurrentMailbox() {
    if (isFetchingMore || isDeepSearchActive || searchQuery.trim()) return;
    const token = mailboxNextPageToken.get(currentMailbox);
    if (!token) return;
    isFetchingMore = true;
    setListFetchIndicator(t("list.loading_more"));
    try {
        const { getActiveAccountInfo } = await import("./api.js");
        const info = await getActiveAccountInfo();
        let next;
        if (info?.provider === "imap") {
            next = await syncImapMailboxPage(currentMailbox, token);
            if (next) {
                mailboxNextPageToken.set(currentMailbox, token + 50);
            } else {
                mailboxNextPageToken.set(currentMailbox, null);
            }
        } else {
            next = await syncMailboxPage(currentMailbox, token);
            mailboxNextPageToken.set(currentMailbox, next || null);
        }
        currentEmails = await getEmails(currentMailbox);
        renderEmailList(false);
        if (!next) {
            setListFetchIndicator(t("list.no_more"));
            setTimeout(() => setListFetchIndicator(""), 1000);
        }
    } catch (error) {
        console.error("Failed to fetch more emails", error);
        setListFetchIndicator("");
    } finally {
        isFetchingMore = false;
        if (mailboxNextPageToken.get(currentMailbox)) setListFetchIndicator("");
    }
}

function bindSearch() {
    const input = document.getElementById("search-input");
    if (!input) return;

    const searchBar = input.closest(".search-bar");
    let deepBtn = document.getElementById("deep-search-btn");
    if (!deepBtn && searchBar) {
        deepBtn = document.createElement("button");
        deepBtn.id = "deep-search-btn";
        deepBtn.className = "deep-search-btn";
        deepBtn.textContent = t("list.search.deep");
        searchBar.appendChild(deepBtn);
        searchBar.classList.add("has-deep-btn");
    }

    const updateDeepButtonVisibility = () => {
        if (deepBtn) deepBtn.hidden = !searchQuery.trim();
    };

    deepBtn?.addEventListener("click", async () => {
        if (!searchQuery.trim()) return;
        deepBtn.disabled = true;
        deepBtn.textContent = t("list.search.searching");
        try {
            const { deepSearchEmails } = await import("./api.js");
            const results = await deepSearchEmails(searchQuery.trim());
            isDeepSearchActive = true;
            currentEmails = results || [];
            renderEmailList(false);
            setListTitle(currentMailbox, currentEmails.length);
        } catch (error) {
            showToast(String(error), "error", 2600);
        } finally {
            deepBtn.disabled = false;
            deepBtn.textContent = t("list.search.deep");
            updateDeepButtonVisibility();
        }
    });

    input.addEventListener("input", () => {
        searchQuery = input.value || "";
        if (!searchQuery.trim()) isDeepSearchActive = false;
        if (currentMailbox === "INBOX") {
            loadInboxThreads(false);
        } else {
            renderEmailList(false);
        }
        updateDeepButtonVisibility();
    });

    updateDeepButtonVisibility();
}

function bindFilterChips() {
    const chips = Array.from(document.querySelectorAll(".filter-chips .chip"));
    chips.forEach((chip) => {
        chip.onclick = () => {
            chips.forEach((c) => c.classList.remove("active"));
            chip.classList.add("active");
            activeFilter = chip.dataset.filter || "All";
            if (currentMailbox === "INBOX") {
                loadInboxThreads(false);
            } else {
                renderEmailList(false);
            }
        };
    });
}



function cycleMailbox(direction = 1) {
    const items = Array.from(document.querySelectorAll(".sidebar .nav-item"));
    if (items.length === 0) return;
    const activeIndex = Math.max(0, items.findIndex((n) => n.classList.contains("active")));
    const nextIndex = (activeIndex + direction + items.length) % items.length;
    const mailbox = items[nextIndex].dataset.mailbox;
    if (!mailbox) return;
    items.forEach((n) => n.classList.remove("active"));
    items[nextIndex].classList.add("active");
    searchQuery = "";
    const input = document.getElementById("search-input");
    if (input) { input.value = ""; input.dispatchEvent(new Event("input")); }
    openMailbox(mailbox, true);
}

function bindHotkeys() {
    document.addEventListener("keydown", async (event) => {
        const hotkeys = getHotkeys();
        const combo = normalizeCombo(eventCombo(event));

        if (combo === hotkeys.close) {
            if (isSettingsOpen()) { closeOverlay(); return; }
            if (isComposeOpen()) { closeCompose(); return; }
            if (selectedEmail || getSelectedThreadId()) {
                selectedEmail = null;
                clearSelectedThread();
                document.querySelectorAll(".email-item").forEach((el) => el.classList.remove("active"));
                setReadingPaneHidden(true);
            }
            return;
        }

        if (!hotkeys.enabled) return;

        if (combo === hotkeys.compose) {
            event.preventDefault();
            if (!canRunHotkey("compose")) return;
            openCompose();
            return;
        }

        if (combo === hotkeys.composeMaximize) {
            if (!isComposeOpen()) return;
            const target = event.target;
            if (target instanceof Element && target.closest("input, textarea, [contenteditable='true']")) return;
            event.preventDefault();
            if (!canRunHotkey("composeMaximize")) return;
            if (typeof window.toggleComposeMaximized === "function") window.toggleComposeMaximized();
            return;
        }

        if (combo === hotkeys.refresh) {
            event.preventDefault();
            if (!canRunHotkey("refresh")) return;
            if (isSyncing) return;
            isSyncing = true;
            showToast(t("toast.fetching"));
            try {
                await loadLocalMailbox(currentMailbox, true);
                await syncMailboxInBackground(currentMailbox, true, onSynced);
            } catch (err) {
                showToast(String(err), "error");
            } finally {
                isSyncing = false;
            }
            return;
        }

        if (combo === hotkeys.settings) {
            event.preventDefault();
            if (!canRunHotkey("settings")) return;
            const profile = await getUserProfile();
            await openSettingsModal(profile, currentMailbox, showOnboardingAndReset, onSync);
            return;
        }

        if (combo === hotkeys.search) {
            event.preventDefault();
            if (!canRunHotkey("search")) return;
            document.getElementById("search-input")?.focus();
        }

        if (hotkeys.nextMailbox && (combo === hotkeys.nextMailbox || combo.replace(/^shift\+/, "") === hotkeys.nextMailbox)) {
            if (isSettingsOpen() || isComposeOpen()) return;
            if (event.target instanceof Element && event.target.closest("input, textarea, select")) return;
            event.preventDefault();
            if (!canRunHotkey("nextMailbox")) return;
            cycleMailbox(1);
            return;
        }

        if (combo === hotkeys.switchNextAccount) {
            event.preventDefault();
            if (!canRunHotkey("switchNextAccount")) return;
            (async () => {
                try {
                    const accounts = await listAccounts();
                    if (!accounts || accounts.length === 0) {
                        showToast(t("accounts.switch_none"));
                        return;
                    }
                    if (accounts.length === 1) {
                        showToast(t("accounts.switch_single"));
                        return;
                    }

                    const currentIndex = accounts.findIndex(acc => acc.is_active);
                    const nextIndex = (currentIndex + 1) % accounts.length;
                    const nextAccount = accounts[nextIndex];

                    const accountLabel = nextAccount.display_name || nextAccount.email;
                    showToast(t("accounts.switched", { account: accountLabel }));

                    await switchActiveAccount(nextAccount.id);
                } catch (err) {
                    showToast(String(err), "error");
                }
            })();
        }
    });
}

function bindGlobalExternalLinkInterception() {
    document.addEventListener("click", async (event) => {
        const target = event.target instanceof Element ? event.target : event.target?.parentElement;
        const anchor = target?.closest?.("a[href]");
        if (!anchor) return;

        const href = anchor.getAttribute("href") || "";
        if (!(href.startsWith("http://") || href.startsWith("https://"))) return;

        event.preventDefault();
        event.stopPropagation();
        try {
            await openExternalUrl(href);
        } catch (error) {
            console.error("Global external link interception failed", error);
        }
    }, true);
}

async function onSync() {
    await syncMailboxInBackground(currentMailbox, true, onSynced);
    await refreshCounts();
}

function showOnboardingAndReset() {
    document.getElementById("root").innerHTML = "";
    initLang();
    showOnboarding(initializeConnectedUI);
}



async function initializeConnectedUI() {
    renderShell();

    bindAppHeaderControls(isComposeOpen, isSettingsOpen, () => currentMailbox);
    bindSidebarCollapse();
    bindMailboxNav(async (mailbox) => {
        searchQuery = "";
        const input = document.getElementById("search-input");
        if (input) { input.value = ""; input.dispatchEvent(new Event("input")); }
        await openMailbox(mailbox, true);
    });
    bindReadingActions(
        () => selectedEmail,
        (v) => { selectedEmail = v; },
        refreshAfterAction,
        openComposeForDraft,
        () => currentMailbox,
        () => getSelectedThreadId(),
        () => getSelectedThreadLatestMessage(),
    );
    bindEmailListContextMenu({
        getMailbox: () => currentMailbox,
        resolveThread: getThreadById,
        resolveEmail: (emailId) => currentEmails.find((e) => e.id === emailId) || null,
        onRefresh: refreshAfterAction,
    });
    bindFilterChips();
    bindSearch();
    bindPaneResizer();
    bindInfiniteScroll();
    bindComposeWindowControls();
    bindComposeRecipientInputs();
    bindComposeFormatting();
    bindComposeAttachments();
    bindComposeSend(async () => { await openMailbox(currentMailbox, false); });
    bindComposeDraftSave(async () => { await openMailbox(currentMailbox, false); });
    bindComposeClear();
    bindHotkeys();
    bindGlobalExternalLinkInterception();

    setReadingPaneHidden(true);
    bindUserRow(() => {
        openAccountPopover(
            handleAccountSwitch,
            handleAfterAddAccount,
        );
    });

    const { listen } = await import("@tauri-apps/api/event");
    await listen("emails-synced", refreshFromSyncedEvent);

    hideBootScreen();

    const profilePromise = getUserProfile();
    const inboxNowPromise = getEmails("INBOX");
    const openPromise = openMailbox("INBOX", true, false);

    const profile = await profilePromise;
    setUserProfile(profile);

    if (profile.degraded) {
        showToast(t("toast.rate_limited"));
        retryUserProfile();
    }

    inboxNowPromise.then(async (inboxNow) => {
        ingestContactsFromEmails(inboxNow);
        const { notifyNewEmails } = await import("./lib/sync.js");
        await notifyNewEmails(inboxNow);
    }).catch(console.error);

    await openPromise;

    window.addEventListener("verdant-open-settings", async () => {
        try {
            const p = await getUserProfile();
            await openSettingsModal(p, currentMailbox, showOnboardingAndReset, onSync);
        } catch {}
    });

    checkAndShowWhatsNewModal().catch(() => {});
    runStartupUpdateCheck().catch(() => {});
    startPeriodicUpdateCheck();
}



document.addEventListener("DOMContentLoaded", async () => {
    initLang();
    showBootScreen();

    const flags = await getStartupFlags().catch(() => ({ is_autostart: false }));
    if (flags.is_autostart) {
        hideMainWindow().catch(err => console.error("Autostart hide failed:", err));
    }

    const { invoke } = await import("@tauri-apps/api/core");
    const [status, _prefsReady, _contactsReady] = await Promise.all([
        authStatus(),
        hydratePrefsFromBackend(),
        ensureContactsLoaded().catch(() => {}),
    ]);
    invoke("update_app_config", { config: { run_in_background: appPrefs.runInBackground, update_channel: updatePrefs.channel } })
        .catch(err => console.error("Initial app config sync failed", err));

    try {
        if (!status.has_client_id) {
            hideBootScreen();
            renderShell();
            document.getElementById("root").innerHTML = `
                <div style="display:flex;align-items:center;justify-content:center;height:100vh;font-family:'DM Sans',sans-serif;color:var(--text-mid);flex-direction:column;gap:12px;">
                    <div style="font:500 15px 'Fraunces',serif;color:var(--text);">${t("app.config_required")}</div>
                    <div style="font-size:13px;">${t("app.config_missing")}</div>
                </div>
            `;
            return;
        }

        if (!status.connected) {
            hideBootScreen();
            showOnboarding(initializeConnectedUI);
            return;
        }

        await initializeConnectedUI();
    } catch (error) {
        hideBootScreen();
        document.getElementById("root").innerHTML = `
            <div style="display:flex;align-items:center;justify-content:center;height:100vh;font-family:'DM Sans',sans-serif;color:var(--text-mid);flex-direction:column;gap:12px;">
                <div style="font:500 15px 'Fraunces',serif;color:var(--text);">${t("app.init_failed")}</div>
                <div style="font-size:13px;">${escapeHtml(String(error))}</div>
                <button onclick="window.location.reload()" style="margin-top:8px;padding:8px 16px;background:var(--green);color:#fff;border:none;border-radius:8px;cursor:pointer;font-family:'DM Sans',sans-serif;">${t("app.retry")}</button>
            </div>
        `;
    }
});
