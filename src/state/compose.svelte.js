import * as api from "../lib/api.js";
import { contactName, normalizeEmailAddress, parseContactToken, parseContactsFromHeader, upsertContact } from "../lib/contacts.js";
import { escapeHtml, sanitizeUnicodeNoise } from "../lib/format.js";
import { t } from "../lib/i18n/index.svelte.js";
import { sanitizeEmailHtmlForCompose } from "../lib/sanitize.js";
import { readJson, writeJson, writeText } from "../lib/storage.js";
import { mail } from "./mail.svelte.js";
import { showToast } from "./ui.svelte.js";

const LOCAL_DRAFT_KEY = "verdant.localDraft";
const SEND_HOLD_MS = 10000;
const EMPTY_BODIES = ["", "<br>", "<div><br></div>"];
const QUOTE_STYLE = "border-left: 3px solid #c8d5c4; padding-left: 12px; margin-top: 16px; color: #4a4d45;";
const QUOTE_META_STYLE = "font-size: 12px; color: #8a8d84; margin-bottom: 8px; line-height: 1.6;";

const rawMessageId = (id) => (id.includes(":") ? id.split(":").slice(1).join(":") : id);

function recipientsFromHeader(header) {
  return parseContactsFromHeader(header).map((contact) => ({
    email: contact.email,
    name: contact.name || contactName(contact.email),
  }));
}

function quoted(email, metaHtml) {
  const original = sanitizeEmailHtmlForCompose(email.body_html || `<pre>${escapeHtml(email.snippet || "")}</pre>`);
  return `<div><br></div><div style="${QUOTE_STYLE}"><div style="${QUOTE_META_STYLE}">${metaHtml}</div><div style="font-size: 13px;">${original}</div></div>`;
}

class ComposeStore {
  open = $state(false);
  maximized = $state(false);
  formatting = $state(false);
  recipients = $state({ to: [], cc: [] });
  pending = $state({ to: "", cc: "" });
  subject = $state("");
  bodyHtml = $state("");
  attachments = $state([]);
  holding = $state(false);
  busy = $state(false);
  focus = $state({ target: "", tick: 0 });

  mode = "plain";
  draftId = null;
  inReplyTo = null;
  editor = null;
  #holdTimer = null;
  #cancelHold = null;

  openNew() {
    if (this.open) return;
    this.#reset();
    const saved = readJson(LOCAL_DRAFT_KEY, null);
    if (saved) {
      this.recipients = { to: recipientsFromHeader(saved.to), cc: recipientsFromHeader(saved.cc) };
      this.subject = saved.subject || "";
      this.bodyHtml = saved.bodyHtml || "";
      this.mode = saved.mode || "html";
    }
    this.#show("to");
  }

  openDraft(email) {
    this.#reset();
    this.recipients = { to: recipientsFromHeader(email.to_recipients), cc: recipientsFromHeader(email.cc_recipients) };
    this.subject = email.subject || "";
    this.bodyHtml = email.body_html || "";
    this.mode = "html";
    this.draftId = email.draft_id || null;
    this.#show("body");
  }

  openReply(email) {
    this.#reset();
    const sender = escapeHtml(sanitizeUnicodeNoise(email.sender || "Unknown"));
    this.recipients.to = recipientsFromHeader(email.sender);
    this.subject = /^re:/i.test((email.subject || "").trim()) ? email.subject : `Re: ${email.subject || ""}`;
    this.bodyHtml = quoted(email, t("compose.quoted_on", { date: escapeHtml(email.date || ""), sender }));
    this.mode = "html";
    this.inReplyTo = rawMessageId(email.id);
    this.#show("body");
  }

  openForward(email) {
    this.#reset();
    const meta = [
      t("compose.forwarded_message"),
      `${t("app.from")}: ${escapeHtml(sanitizeUnicodeNoise(email.sender || "Unknown"))}`,
      `${t("app.date")}: ${escapeHtml(email.date || "")}`,
      `${t("app.subject")}: ${escapeHtml(sanitizeUnicodeNoise(email.subject || ""))}`,
      `${t("app.to")}: ${escapeHtml(sanitizeUnicodeNoise(email.to_recipients || ""))}`,
    ].join("<br>");
    this.subject = /^fwd:/i.test((email.subject || "").trim()) ? email.subject : `Fwd: ${email.subject || ""}`;
    this.bodyHtml = quoted(email, meta);
    this.mode = "html";
    this.#show("to");
  }

  close() {
    if (!this.open) return;
    this.cancelSend();
    if (!this.draftId && !this.inReplyTo) this.#saveLocalDraft();
    this.open = false;
    this.#reset();
  }

  clear() {
    writeText(LOCAL_DRAFT_KEY, null);
    this.#reset();
  }

  toggleFormatting() {
    this.formatting = !this.formatting;
    if (this.formatting) this.mode = "html";
  }

  addRecipient(field, contactLike) {
    const parsed = typeof contactLike === "string"
      ? parseContactToken(contactLike)
      : { email: normalizeEmailAddress(contactLike?.email || ""), name: sanitizeUnicodeNoise(contactLike?.name || "") };
    if (!parsed?.email || this.recipients[field].some((r) => r.email === parsed.email)) return false;

    const recipient = { email: parsed.email, name: parsed.name || contactName(parsed.email) };
    this.recipients[field].push(recipient);
    upsertContact(recipient.email, recipient.name);
    this.pending[field] = "";
    return true;
  }

  removeRecipient(field, index) {
    this.recipients[field].splice(index, 1);
  }

  commitPending(field) {
    const raw = sanitizeUnicodeNoise(this.pending[field] || "");
    if (!raw) return;
    const contacts = parseContactsFromHeader(raw);
    if (contacts.length) contacts.forEach((contact) => this.addRecipient(field, contact));
    else this.addRecipient(field, raw);
  }

  addAttachments(attachments) {
    this.attachments.push(...attachments);
  }

  removeAttachment(index) {
    this.attachments.splice(index, 1);
  }

  async send() {
    if (this.busy) return;
    const payload = this.#payload();
    if (!payload.to) {
      showToast(t("toast.recipient_required"), "error");
      return;
    }

    this.busy = true;
    try {
      await this.#hold();
      if (this.draftId) {
        const saved = await api.saveDraft({ ...payload, draftId: this.draftId });
        await api.sendExistingDraft(saved.draft_id || this.draftId);
      } else {
        await api.sendEmail({ ...payload, inReplyTo: this.inReplyTo, references: this.inReplyTo });
      }
      for (const field of ["to", "cc"]) {
        parseContactsFromHeader(payload[field]).forEach((contact) => upsertContact(contact.email, contact.name));
      }
      showToast(t("toast.sent"));
      writeText(LOCAL_DRAFT_KEY, null);
      this.open = false;
      this.#reset();
      await mail.open(mail.mailbox, { animate: false });
    } catch (error) {
      if (error?.message !== "cancelled") showToast(String(error), "error", 4000);
    } finally {
      this.busy = false;
      this.holding = false;
    }
  }

  cancelSend() {
    this.#cancelHold?.();
  }

  async saveDraft() {
    showToast(t("toast.draft_saving"));
    try {
      const result = await api.saveDraft({ ...this.#payload(), draftId: this.draftId });
      this.draftId = result.draft_id || this.draftId;
      showToast(t("toast.draft_saved"));
      await mail.open(mail.mailbox, { animate: false });
    } catch (error) {
      showToast(String(error), "error", 4000);
    }
  }

  #show(target) {
    this.open = true;
    this.focus = { target, tick: this.focus.tick + 1 };
  }

  #reset() {
    this.recipients = { to: [], cc: [] };
    this.pending = { to: "", cc: "" };
    this.subject = "";
    this.bodyHtml = "";
    this.attachments = [];
    this.formatting = false;
    this.mode = "plain";
    this.draftId = null;
    this.inReplyTo = null;
  }

  #payload() {
    this.commitPending("to");
    this.commitPending("cc");
    const html = this.editor?.innerHTML ?? this.bodyHtml;
    return {
      to: this.recipients.to.map((r) => r.email).join(", "),
      cc: this.recipients.cc.map((r) => r.email).join(", "),
      subject: this.subject.trim(),
      body: this.editor?.innerText ?? "",
      mode: this.mode,
      bodyHtml: this.mode === "html" && !EMPTY_BODIES.includes(html.trim()) ? html : null,
      attachments: $state.snapshot(this.attachments),
    };
  }

  #saveLocalDraft() {
    const payload = this.#payload();
    if (!payload.to && !payload.subject && !payload.body.trim()) {
      writeText(LOCAL_DRAFT_KEY, null);
      return;
    }
    writeJson(LOCAL_DRAFT_KEY, {
      to: payload.to,
      cc: payload.cc,
      subject: payload.subject,
      bodyHtml: this.editor?.innerHTML ?? this.bodyHtml,
      mode: this.mode,
    });
  }

  #hold() {
    this.holding = true;
    return new Promise((resolve, reject) => {
      const finish = (settle) => {
        clearTimeout(this.#holdTimer);
        this.#cancelHold = null;
        this.holding = false;
        settle();
      };
      this.#holdTimer = setTimeout(() => finish(resolve), SEND_HOLD_MS);
      this.#cancelHold = () => finish(() => reject(new Error("cancelled")));
    });
  }
}

export const compose = new ComposeStore();
