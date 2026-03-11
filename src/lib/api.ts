import { invoke } from "@tauri-apps/api/tauri";

export interface Account {
  id: string;
  name: string;
  email: string;
  imap_host: string;
  imap_port: number;
  imap_tls: boolean;
  smtp_host: string;
  smtp_port: number;
  smtp_tls: boolean;
}

export interface AddAccountInput {
  name: string;
  email: string;
  password: string;
  imap_host: string;
  imap_port: number;
  imap_tls: boolean;
  smtp_host: string;
  smtp_port: number;
  smtp_tls: boolean;
}

export interface Mailbox {
  id: string;
  account_id: string;
  name: string;
  full_name: string;
  flags: string;
}

export interface Message {
  id: string;
  account_id: string;
  mailbox_id: string;
  uid: number | null;
  message_id: string | null;
  subject: string;
  sender_name: string;
  sender_email: string;
  recipients: string;
  date_str: string;
  date_ts: number;
  flags: string;
  preview: string;
  body_text: string | null;
  body_html: string | null;
  in_reply_to: string | null;
  references_hdr: string | null;
  has_attachments: boolean;
  headers_fetched: boolean;
  body_fetched: boolean;
}

export interface OutgoingMessage {
  from_name: string;
  from_email: string;
  to: string[];
  cc: string[];
  subject: string;
  body: string;
  in_reply_to?: string;
  references?: string;
}

export const listAccounts = () => invoke<Account[]>("list_accounts");

export const addAccount = (input: AddAccountInput) =>
  invoke<Account>("add_account", { input });

export const deleteAccount = (id: string) =>
  invoke<void>("delete_account", { id });

export const listMailboxes = (accountId: string) =>
  invoke<Mailbox[]>("list_mailboxes", { accountId: accountId });

export const listMessages = (
  mailboxId: string,
  limit = 50,
  offset = 0
) => invoke<Message[]>("list_messages", { mailboxId, limit, offset });

export const getMessage = (id: string) =>
  invoke<Message>("get_message", { id });

export const fetchMessageBody = (messageId: string) =>
  invoke<Message>("fetch_message_body", { messageId });

export const syncAccount = (accountId?: string) =>
  invoke<void>("sync_account", { accountId });

export const searchMessages = (query: string, accountId?: string) =>
  invoke<Message[]>("search_messages", { query, accountId });

export const sendEmail = (accountId: string, msg: OutgoingMessage) =>
  invoke<void>("send_email", { accountId, msg });

export const saveDraft = (
  accountId: string,
  mailboxId: string,
  msg: OutgoingMessage
) => invoke<string>("save_draft", { accountId, mailboxId, msg });

export const markRead = (id: string) => invoke<void>("mark_read", { id });

export function parseFlags(flagsJson: string): string[] {
  try {
    return JSON.parse(flagsJson);
  } catch {
    return [];
  }
}

export function isUnread(msg: Message): boolean {
  const flags = parseFlags(msg.flags);
  return !flags.some((f) => f.toLowerCase().includes("seen"));
}

export function formatDate(ts: number): string {
  if (!ts) return "";
  const d = new Date(ts * 1000);
  const now = new Date();
  const diffDays = Math.floor(
    (now.getTime() - d.getTime()) / (1000 * 60 * 60 * 24)
  );
  if (diffDays === 0) {
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  } else if (diffDays === 1) {
    return "Yesterday";
  } else if (diffDays < 7) {
    return d.toLocaleDateString([], { weekday: "short" });
  }
  return d.toLocaleDateString([], { month: "short", day: "numeric" });
}

export function senderInitials(msg: Message): string {
  const name = msg.sender_name || msg.sender_email || "?";
  return name
    .split(/\s+/)
    .map((w) => w[0]?.toUpperCase() ?? "")
    .slice(0, 2)
    .join("");
}
