import { SvelteMap } from "svelte/reactivity";
import * as api from "../lib/api.js";
import { hasAttachments } from "../lib/attachments.js";
import { ingestContactsFromEmails } from "../lib/contacts.js";
import { t } from "../lib/i18n/index.svelte.js";
import { findVerificationCode, mentionsCode } from "../lib/verificationCode.js";
import { selection } from "./selection.svelte.js";
import { categoryLabel, smart } from "./smartInbox.svelte.js";
import { showToast } from "./ui.svelte.js";

const ANIMATION_WINDOW_MS = 1800;
const RESYNC_COOLDOWN_MS = 10 * 1000;
const SYNC_TIMEOUT_MS = 8 * 1000;
const IMAP_FAILURE_COOLDOWN_MS = 5 * 60 * 1000;
const IMAP_PAGE_SIZE = 50;
const MAX_CODE_LOOKUPS = 30;
const DUST_WINDOW_MS = 1500;
const BULK_LABELS = ["SPAM", "TRASH", "CATEGORY_PROMOTIONS"];
const EMPTY_COUNTS = {
  inbox_total: 0,
  inbox_unread: 0,
  starred_total: 0,
  sent_total: 0,
  drafts_total: 0,
  archive_total: 0,
  trash_total: 0,
};

export const CATEGORY_PREFIX = "CATEGORY:";

const isThreadBox = (mailbox) => mailbox === "INBOX" || mailbox.startsWith(CATEGORY_PREFIX);
const serverMailbox = (mailbox) => (mailbox.startsWith(CATEGORY_PREFIX) ? "INBOX" : mailbox);
const isBulk = (labels) => (labels || "").split(",").some((label) => BULK_LABELS.includes(label.trim()));
const codeKey = (thread) => `${thread.thread_id}:${thread.latest_ts || ""}`;

function withTimeout(promise, message) {
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(message)), SYNC_TIMEOUT_MS);
  });
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
}

function whenIdle(work) {
  if (typeof window.requestIdleCallback === "function") window.requestIdleCallback(work, { timeout: 250 });
  else setTimeout(work, 0);
}

class MailStore {
  mailbox = $state("INBOX");
  filter = $state("Important");
  search = $state("");
  threads = $state({});
  emails = $state({});
  deepResults = $state(null);
  deepSearching = $state(false);
  loading = $state(false);
  animate = $state(false);
  activeSyncs = $state(0);
  fetchNote = $state("");
  counts = $state({ ...EMPTY_COUNTS });
  selectedThreadId = $state(null);
  selectedEmail = $state(null);
  conversation = $state({ messages: [], loading: false, error: "" });

  codes = new SvelteMap();
  dusting = new Set();

  #lastSynced = new Map();
  #nextPage = new Map();
  #imapDownUntil = new Map();
  #animateTimer = null;
  #fetchingMore = false;
  #codeScan = 0;

  isThreadView = $derived(this.deepResults === null && isThreadBox(this.mailbox));

  categorySlug = $derived(
    this.mailbox.startsWith(CATEGORY_PREFIX) ? this.mailbox.slice(CATEGORY_PREFIX.length) : null,
  );

  rows = $derived.by(() => {
    const query = this.search.trim().toLowerCase();
    const matches = (...fields) => !query || fields.join(" ").toLowerCase().includes(query);

    if (this.isThreadView) {
      return (this.threads[this.mailbox] ?? [])
        .filter((thread) => {
          if (this.filter === "Important" && isBulk(thread.labels)) return false;
          if (this.filter === "Unread" && thread.is_read) return false;
          if (this.filter === "Attachments" && !thread.has_attachments) return false;
          return matches(thread.subject, thread.participants, thread.snippet);
        })
        .map((thread) => ({ key: thread.thread_id, thread }));
    }

    const skipImportance = this.mailbox === "TRASH" || this.mailbox === "SPAM";
    return (this.deepResults ?? this.emails[this.mailbox] ?? [])
      .filter((email) => {
        if (this.filter === "Important" && !skipImportance && isBulk(email.labels)) return false;
        if (this.filter === "Unread" && email.is_read) return false;
        if (this.filter === "Attachments" && !hasAttachments(email)) return false;
        return matches(email.subject || "", email.sender || "", email.snippet || "");
      })
      .map((email) => ({ key: email.id, email }));
  });

  selectedThread = $derived(
    this.selectedThreadId
      ? ((this.threads[this.mailbox] ?? []).find((thread) => thread.thread_id === this.selectedThreadId) ?? null)
      : null,
  );

  selectedRow = $derived.by(() => {
    if (this.selectedThread) return { key: this.selectedThread.thread_id, thread: this.selectedThread };
    if (this.selectedEmail) return { key: this.selectedEmail.id, email: this.selectedEmail };
    return null;
  });

  hasSelection = $derived(this.selectedThreadId !== null || this.selectedEmail !== null);

  rowByKey(key) {
    return this.rows.find((row) => row.key === key) ?? null;
  }

  codeFor(thread) {
    return this.codes.get(codeKey(thread)) ?? null;
  }

  reset() {
    this.threads = {};
    this.emails = {};
    this.deepResults = null;
    this.search = "";
    this.counts = { ...EMPTY_COUNTS };
    this.#nextPage.clear();
    this.clearSelection();
    selection.exit();
  }

  clearSelection() {
    this.selectedThreadId = null;
    this.selectedEmail = null;
    this.conversation = { messages: [], loading: false, error: "" };
  }

  setSearch(value) {
    this.search = value;
    if (!value.trim()) this.deepResults = null;
  }

  async refreshCounts() {
    try {
      this.counts = await api.getMailboxCounts();
    } catch (error) {
      console.error("Failed to load mailbox counts", error);
    }
    smart.refreshSoon();
  }

  async open(mailbox, { animate = true, sync = true } = {}) {
    if (mailbox !== this.mailbox) {
      this.clearSelection();
      this.deepResults = null;
      this.search = "";
      selection.exit();
    }
    this.mailbox = mailbox;
    if (animate) this.#startAnimation();

    const loaded = this.#load(mailbox);
    const synced = sync ? this.sync(mailbox, true).catch((error) => console.warn("Sync unavailable; using local cache", error)) : null;
    await loaded;
    await synced;
  }

  async refresh() {
    this.#startAnimation();
    await this.reload();
    await this.sync(this.mailbox, true);
  }

  async reload(mailbox = this.mailbox) {
    try {
      if (isThreadBox(mailbox)) {
        this.threads[mailbox] = await this.#fetchThreads(mailbox);
      } else {
        const emails = await api.getEmails(mailbox);
        this.emails[mailbox] = emails;
        whenIdle(() => ingestContactsFromEmails(emails));
      }
    } catch (error) {
      console.error(`Failed to load ${mailbox}`, error);
    }
    this.refreshCounts();
  }

  async sync(mailbox = this.mailbox, force = false) {
    const now = Date.now();
    if (!force && now - (this.#lastSynced.get(mailbox) || 0) < RESYNC_COOLDOWN_MS) return;
    this.#lastSynced.set(mailbox, now);

    const target = serverMailbox(mailbox);
    this.activeSyncs += 1;
    try {
      const account = await api.getActiveAccountInfo();
      if (account?.provider === "imap") {
        await this.#syncImap(target);
      } else {
        await withTimeout(api.syncMailbox(target), `Mailbox sync timed out for ${target}`);
        if (target === "INBOX") {
          const next = await withTimeout(api.syncMailboxPage("INBOX", null), "Inbox reconciliation timed out");
          if (!this.#nextPage.has("INBOX")) this.#nextPage.set("INBOX", next || null);
        }
      }
      await this.reload(mailbox);
    } finally {
      this.activeSyncs -= 1;
    }
  }

  lastSyncedAt(mailbox) {
    return this.#lastSynced.get(mailbox) || 0;
  }

  async fetchMore() {
    if (this.#fetchingMore || this.deepResults || this.search.trim()) return;
    const target = serverMailbox(this.mailbox);
    const cursor = this.#nextPage.get(target);
    if (!cursor) return;

    this.#fetchingMore = true;
    this.fetchNote = t("list.loading_more");
    try {
      const account = await api.getActiveAccountInfo();
      let next;
      if (account?.provider === "imap") {
        next = (await api.syncImapMailboxPage(target, cursor)) ? cursor + IMAP_PAGE_SIZE : null;
      } else {
        next = (await api.syncMailboxPage(target, cursor)) || null;
      }
      this.#nextPage.set(target, next);
      await this.reload();
      this.fetchNote = next ? "" : t("list.no_more");
      if (!next) setTimeout(() => (this.fetchNote = ""), 1000);
    } catch (error) {
      console.error("Failed to fetch more emails", error);
      this.fetchNote = "";
    } finally {
      this.#fetchingMore = false;
    }
  }

  async deepSearch() {
    const query = this.search.trim();
    if (!query || this.deepSearching) return;
    this.deepSearching = true;
    try {
      const results = await api.deepSearchEmails(query);
      this.clearSelection();
      this.deepResults = results || [];
    } catch (error) {
      showToast(String(error), "error", 2600);
    } finally {
      this.deepSearching = false;
    }
  }

  async selectThread(thread) {
    const threadId = thread.thread_id;
    this.selectedEmail = null;
    this.selectedThreadId = threadId;
    this.conversation = { messages: [], loading: true, error: "" };

    try {
      const messages = await api.getThreadMessages(threadId);
      if (this.selectedThreadId !== threadId) return;
      this.conversation = { messages, loading: false, error: "" };

      const unread = this.conversation.messages.filter((message) => !message.is_read);
      if (!unread.length && thread.is_read) return;
      unread.forEach((message) => (message.is_read = true));
      thread.is_read = true;
      thread.unread_count = 0;
      api
        .markEmailsRead(unread.map((message) => message.id))
        .catch((error) => console.error("Failed to mark conversation as read", error))
        .finally(() => this.refreshCounts());
    } catch (error) {
      if (this.selectedThreadId === threadId) {
        this.conversation = { messages: [], loading: false, error: String(error) };
      }
    }
  }

  async selectEmail(email) {
    this.selectedThreadId = null;
    this.conversation = { messages: [], loading: false, error: "" };
    this.selectedEmail = email;
    if (!email.body_html) {
      const full = await api.getEmail(email.id).catch((error) => console.error("Failed to load email", error));
      if (full) email.body_html = full.body_html;
    }
    if (email.is_read) return;
    email.is_read = true;
    await api.setEmailReadStatus(email.id, true).catch((error) => console.error("Failed to mark as read", error));
    await this.refreshCounts();
  }

  selectRow(row) {
    return row.thread ? this.selectThread(row.thread) : this.selectEmail(row.email);
  }

  async archive(rows) {
    if (await this.#applyToMessages(rows, api.archiveEmail)) this.#removed(rows, t("toast.archived"));
  }

  async restore(rows) {
    if (await this.#applyToMessages(rows, api.restoreFromTrash)) this.#removed(rows, t("toast.restored"));
  }

  async moveToInbox(rows) {
    if (await this.#applyToMessages(rows, api.moveToInbox)) this.#removed(rows, t("toast.moved_to_inbox"));
  }

  async trash(rows) {
    const permanent = this.mailbox === "TRASH" || rows.every((row) => row.email?.mailbox === "TRASH");
    const done = await this.#applyToMessages(rows, permanent ? api.permanentDeleteEmail : api.trashEmail);
    if (done) this.#removed(rows, t(permanent ? "toast.permanently_deleted" : "toast.trashed"));
  }

  async markRead(rows, isRead) {
    await this.#applyToMessages(rows, (id) => api.setEmailReadStatus(id, isRead));
    for (const row of rows) {
      (row.thread ?? row.email).is_read = isRead;
      if (row.thread?.thread_id === this.selectedThreadId) {
        this.conversation.messages.forEach((message) => (message.is_read = isRead));
      }
    }
    showToast(t(isRead ? "toast.read_marked" : "toast.unread_marked"));
    await this.#changed();
  }

  async toggleStar(rows) {
    const starred = !rows.every((row) => (row.thread ?? row.email).starred);
    const differing = rows.filter((row) => !!(row.thread ?? row.email).starred !== starred);
    differing.forEach((row) => ((row.thread ?? row.email).starred = starred));
    showToast(t("toast.star_updated"));
    await this.#applyToMessages(differing, api.toggleStarred);
    if (this.mailbox === "STARRED" && !starred) this.#removed(differing);
    else await this.#changed();
  }

  async moveToCategory(rows, category) {
    const ids = await this.#messageIds(rows);
    if (!ids.length) return;
    try {
      await api.moveEmailsToCategory(ids, category.slug);
    } catch (error) {
      showToast(String(error), "error");
      return;
    }
    const message = t("toast.moved_to_category", { category: categoryLabel(category) });
    if (this.categorySlug && this.categorySlug !== category.slug) this.#removed(rows, message);
    else {
      showToast(message);
      await this.#changed();
    }
  }

  async unsubscribe(email) {
    try {
      await api.unsubscribeFromList(email.id);
      email.unsubscribed = true;
      showToast(t("toast.unsubscribed"));
    } catch (error) {
      console.error("Unsubscribe failed", error);
      showToast(t("toast.unsubscribe_failed"), "error");
    }
  }

  async sendDraft(email) {
    if (!email.draft_id) {
      showToast(t("toast.draft_no_id"), "error");
      return;
    }
    try {
      await api.sendExistingDraft(email.draft_id);
      this.#removed([{ key: email.id, email }], t("toast.draft_sent"));
    } catch (error) {
      showToast(String(error), "error");
    }
  }

  async #load(mailbox) {
    const cached = isThreadBox(mailbox) ? this.threads[mailbox] : this.emails[mailbox];
    if (cached) {
      this.refreshCounts();
      return;
    }
    this.loading = true;
    try {
      await this.reload(mailbox);
    } finally {
      this.loading = false;
    }
  }

  async #fetchThreads(mailbox) {
    const threads = mailbox === "INBOX"
      ? await api.getInboxThreads()
      : await api.getCategoryThreads(mailbox.slice(CATEGORY_PREFIX.length));
    this.#scanCodes(threads);
    return threads;
  }

  async #syncImap(target) {
    if (Date.now() < (this.#imapDownUntil.get(target) || 0)) return;
    try {
      const hasMore = await withTimeout(api.syncImapMailboxPage(target, 0), `IMAP sync timed out for ${target}`);
      if (!this.#nextPage.has(target)) this.#nextPage.set(target, hasMore ? IMAP_PAGE_SIZE : null);
    } catch {
      this.#imapDownUntil.set(target, Date.now() + IMAP_FAILURE_COOLDOWN_MS);
    }
  }

  #startAnimation() {
    this.animate = true;
    clearTimeout(this.#animateTimer);
    this.#animateTimer = setTimeout(() => (this.animate = false), ANIMATION_WINDOW_MS);
  }

  #scanCodes(threads) {
    const scan = ++this.#codeScan;
    const pending = [];
    for (const thread of threads) {
      const key = codeKey(thread);
      if (this.codes.has(key)) continue;
      const code = findVerificationCode(thread);
      if (code) this.codes.set(key, code);
      else if (mentionsCode(thread)) pending.push(thread);
      else this.codes.set(key, null);
    }
    this.#lookUpCodes(pending.slice(0, MAX_CODE_LOOKUPS), scan);
  }

  async #lookUpCodes(threads, scan) {
    for (const thread of threads) {
      if (scan !== this.#codeScan) return;
      try {
        const latest = (await api.getThreadMessages(thread.thread_id)).at(-1);
        const code = latest
          ? findVerificationCode({ subject: latest.subject, snippet: latest.snippet, body: latest.body_html })
          : null;
        this.codes.set(codeKey(thread), code);
      } catch {}
    }
  }

  async #messageIds(rows) {
    const perRow = await Promise.all(
      rows.map(async (row) => {
        if (row.email) return [row.email.id];
        if (row.thread.thread_id === this.selectedThreadId && this.conversation.messages.length) {
          return this.conversation.messages.map((message) => message.id);
        }
        try {
          return (await api.getThreadMessages(row.thread.thread_id)).map((message) => message.id);
        } catch {
          return [];
        }
      }),
    );
    return perRow.flat();
  }

  async #applyToMessages(rows, action) {
    const ids = await this.#messageIds(rows);
    const results = await Promise.allSettled(ids.map((id) => action(id)));
    const failures = results.filter((result) => result.status === "rejected");
    if (ids.length && failures.length === ids.length) {
      showToast(String(failures[0].reason), "error", 4000);
      return false;
    }
    return true;
  }

  #invalidateOtherViews() {
    for (const key of Object.keys(this.threads)) if (key !== this.mailbox) delete this.threads[key];
    for (const key of Object.keys(this.emails)) if (key !== this.mailbox) delete this.emails[key];
  }

  async #changed() {
    this.#invalidateOtherViews();
    await this.reload();
  }

  #removed(rows, message = "") {
    if (message) showToast(message);
    const keys = new Set(rows.map((row) => row.key));
    keys.forEach((key) => {
      this.dusting.add(key);
      selection.keys.delete(key);
    });
    setTimeout(() => keys.forEach((key) => this.dusting.delete(key)), DUST_WINDOW_MS);

    if (this.deepResults) {
      this.deepResults = this.deepResults.filter((email) => !keys.has(email.id));
    } else if (isThreadBox(this.mailbox)) {
      this.threads[this.mailbox] = (this.threads[this.mailbox] ?? []).filter((thread) => !keys.has(thread.thread_id));
    } else {
      this.emails[this.mailbox] = (this.emails[this.mailbox] ?? []).filter((email) => !keys.has(email.id));
    }

    if (keys.has(this.selectedThreadId) || keys.has(this.selectedEmail?.id)) this.clearSelection();
    this.#invalidateOtherViews();
    this.refreshCounts();
    this.sync(this.mailbox).catch(() => {});
  }
}

export const mail = new MailStore();
