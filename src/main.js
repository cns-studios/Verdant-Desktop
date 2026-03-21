import { authStatus, getUserProfile, getEmails, syncMailboxPage } from "./api.js";
import { setEmailReadStatus } from "./api.js";
import { ingestContactsFromEmails } from "./lib/contacts.js";
import { loadHotkeys, saveHotkeys, normalizeCombo, eventCombo, canRunHotkey } from "./lib/hotkeys.js";
import { showToast } from "./lib/toast.js";
import { escapeHtml, sanitizeUnicodeNoise, formatListDate, mailboxTitle } from "./lib/format.js";
import { syncMailboxInBackground, startPeriodicSync, mailboxNextPageToken, knownInboxIds, setKnownInboxIds } from "./lib/sync.js";
import { ensureStyles } from "./ui/styles.js";
import { renderShell } from "./ui/shell.js";
import { showOnboarding } from "./ui/onboarding.js";
import {
  bindMailboxNav, bindPaneResizer, bindAppHeaderControls,
  refreshCounts, setUserProfile, bindUserRow, setListTitle, refreshAppHeaderSubtitle,
} from "./ui/sidebar.js";
import {
  renderReadingPane, bindReadingActions, setReadingPaneHidden,
  applySenderAvatar, hasEmailAttachments, updateTopActionStates,
} from "./ui/reading.js";
import {
  isComposeOpen, openCompose, closeCompose, openComposeForDraft,
  bindComposeRecipientInputs, bindComposeFormatting, bindComposeAttachments,
  bindComposeWindowControls, bindComposeSend, bindComposeDraftSave,
} from "./ui/compose.js";
import {
  openSettingsModal, isSettingsOpen, closeOverlay,
  runAutomaticUpdateFlow, updatePrefs,
} from "./ui/settings.js";

let currentMailbox = "INBOX";
let currentEmails = [];
let selectedEmail = null;
let activeFilter = "Important";
let searchQuery = "";
let isDeepSearchActive = false;
let isFetchingMore = false;
let hotkeys = loadHotkeys();

function isImportant(email) {
  const labels = (email.labels || "").split(",");
  return !labels.includes("CATEGORY_PROMOTIONS") && !labels.includes("SPAM");
}

function emailMatchesFilter(email) {
  if (activeFilter === "Important" && !isImportant(email)) return false;
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
  let selectedRow = null;
  let selectedRowEmail = null;

  for (const email of emails) {
    const row = document.createElement("div");
    row.className = `email-item ${email.is_read ? "" : "unread"}`.trim();
    row.innerHTML = `
      ${email.is_read ? "" : '<div class="unread-dot"></div>'}
      <div class="email-item-main">
        <div class="sender-avatar"></div>
        <div class="email-item-inner">
          <div class="email-top">
            <span class="email-sender">${escapeHtml(sanitizeUnicodeNoise(email.sender || "Unknown Sender"))}</span>
            <span class="email-time">${escapeHtml(formatListDate(email.date))}</span>
          </div>
          <div class="email-subject">${escapeHtml(sanitizeUnicodeNoise(email.subject || "(No Subject)"))}</div>
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
    renderReadingPane(selectedRowEmail);
  } else if (!selectedEmail && emails.length > 0) {
    const first = list.querySelector(".email-item");
    if (first) selectEmail(emails[0], first);
  }
}

async function selectEmail(email, row) {
  setReadingPaneHidden(false);
  selectedEmail = email;
  document.querySelectorAll(".email-item").forEach((el) => el.classList.remove("active"));
  row.classList.add("active");
  row.classList.remove("unread");
  row.querySelector(".unread-dot")?.remove();
  renderReadingPane(email);
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
    isDeepSearchActive = false;
  }
  currentMailbox = mailbox;
  currentEmails = await getEmails(mailbox);
  ingestContactsFromEmails(currentEmails);
  renderEmailList(animate);
  refreshAppHeaderSubtitle(currentMailbox, isComposeOpen, isSettingsOpen);
  await refreshCounts();
}

async function openMailbox(mailbox, animate = false) {
  await loadLocalMailbox(mailbox, animate);
  syncMailboxInBackground(mailbox, false, onSynced).catch((err) => {
    console.error("Background sync failed:", err);
    showToast(String(err), "error", 2500);
  });
}

function onSynced(mailbox, latestEmails) {
  if (currentMailbox === mailbox) {
    currentEmails = latestEmails;
    renderEmailList(false);
    refreshCounts().catch(console.error);
  }
}

async function refreshAfterAction() {
  await loadLocalMailbox(currentMailbox, false);
  syncMailboxInBackground(currentMailbox, false, onSynced).catch(() => {});
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
  setListFetchIndicator("Loading more emails...");
  try {
    const next = await syncMailboxPage(currentMailbox, token);
    mailboxNextPageToken.set(currentMailbox, next || null);
    currentEmails = await getEmails(currentMailbox);
    renderEmailList(false);
    if (!next) {
      setListFetchIndicator("No more emails");
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
    deepBtn.textContent = "Deep Search";
    searchBar.appendChild(deepBtn);
    searchBar.classList.add("has-deep-btn");
  }

  const updateDeepButtonVisibility = () => {
    if (deepBtn) deepBtn.hidden = !searchQuery.trim();
  };

  deepBtn?.addEventListener("click", async () => {
    if (!searchQuery.trim()) return;
    deepBtn.disabled = true;
    deepBtn.textContent = "Searching...";
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
      deepBtn.textContent = "Deep Search";
      updateDeepButtonVisibility();
    }
  });

  input.addEventListener("input", () => {
    searchQuery = input.value || "";
    if (!searchQuery.trim()) isDeepSearchActive = false;
    renderEmailList(false);
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
      activeFilter = chip.dataset.filter || chip.textContent.trim();
      renderEmailList(false);
    };
  });
}

function bindHotkeys() {
  document.addEventListener("keydown", async (event) => {
    const combo = normalizeCombo(eventCombo(event));

    if (combo === hotkeys.close) {
      if (isSettingsOpen()) { closeOverlay(); return; }
      if (isComposeOpen()) { closeCompose(); }
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
      showToast("Fetching mails...");
      await syncMailboxInBackground(currentMailbox, true, onSynced);
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
  });
}

async function onSync() {
  await syncMailboxInBackground(currentMailbox, true, onSynced);
  await refreshCounts();
}

function showOnboardingAndReset() {
  document.getElementById("root").innerHTML = "";
  showOnboarding(initializeConnectedUI);
}

async function initializeConnectedUI() {
  renderShell();

  bindAppHeaderControls(isComposeOpen, isSettingsOpen, () => currentMailbox);
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
    openComposeForDraft
  );
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
  bindHotkeys();

  const profile = await getUserProfile();
  setUserProfile(profile);
  bindUserRow(() =>
    openSettingsModal(profile, currentMailbox, showOnboardingAndReset, onSync).catch(console.error)
  );

  const inboxNow = await getEmails("INBOX");
  ingestContactsFromEmails(inboxNow);
  setKnownInboxIds(new Set((inboxNow || []).map((m) => m.id)));

  await openMailbox("INBOX", true);
  startPeriodicSync(onSynced);
}

document.addEventListener("DOMContentLoaded", async () => {
  ensureStyles();

  try {
    const status = await authStatus();

    if (!status.has_client_id) {
      renderShell();
      const { showOverlay } = await import("./ui/settings.js");
      document.getElementById("root").innerHTML = `
        <div style="display:flex;align-items:center;justify-content:center;height:100vh;font-family:'DM Sans',sans-serif;color:var(--text-mid);flex-direction:column;gap:12px;">
          <div style="font:500 15px 'Fraunces',serif;color:var(--text);">Configuration Required</div>
          <div style="font-size:13px;">Missing GOOGLE_CLIENT_ID in .env. Add credentials and restart Verdant.</div>
        </div>
      `;
      return;
    }

    runAutomaticUpdateFlow().catch((error) => {
      console.warn("Automatic update check failed", error);
    });

    if (!status.connected) {
      showOnboarding(initializeConnectedUI);
      return;
    }

    await initializeConnectedUI();
  } catch (error) {
    document.getElementById("root").innerHTML = `
      <div style="display:flex;align-items:center;justify-content:center;height:100vh;font-family:'DM Sans',sans-serif;color:var(--text-mid);flex-direction:column;gap:12px;">
        <div style="font:500 15px 'Fraunces',serif;color:var(--text);">Initialization Failed</div>
        <div style="font-size:13px;">${escapeHtml(String(error))}</div>
        <button onclick="window.location.reload()" style="margin-top:8px;padding:8px 16px;background:var(--green);color:#fff;border:none;border-radius:8px;cursor:pointer;font-family:'DM Sans',sans-serif;">Retry</button>
      </div>
    `;
  }
});
