import { writable, derived, get } from "svelte/store";
import type { Account, Mailbox } from "../api";
import { listAccounts, listMailboxes, addAccount as apiAddAccount, deleteAccount as apiDeleteAccount } from "../api";
import type { AddAccountInput } from "../api";

export const accounts = writable<Account[]>([]);
export const activeAccountId = writable<string | null>(null);
export const mailboxes = writable<Mailbox[]>([]);
export const activeMailboxId = writable<string | null>(null);
export const addingAccount = writable(false);

export const activeAccount = derived(
  [accounts, activeAccountId],
  ([$accounts, $activeAccountId]) =>
    $accounts.find((a) => a.id === $activeAccountId) ?? null
);

export async function loadAccounts() {
  const list = await listAccounts();
  accounts.set(list);
  if (list.length > 0) {
    const current = get(activeAccountId);
    if (!current || !list.find((a) => a.id === current)) {
      activeAccountId.set(list[0].id);
    }
    await loadMailboxes(get(activeAccountId)!);
  }
}

export async function loadMailboxes(accountId: string) {
  const list = await listMailboxes(accountId);
  mailboxes.set(list);
  // Auto-select INBOX
  const inbox = list.find((m) =>
    m.full_name.toLowerCase() === "inbox" || m.name.toLowerCase() === "inbox"
  );
  if (inbox) {
    activeMailboxId.set(inbox.id);
  } else if (list.length > 0) {
    activeMailboxId.set(list[0].id);
  }
}

export async function addAccount(input: AddAccountInput) {
  const account = await apiAddAccount(input);
  accounts.update((a) => [...a, account]);
  activeAccountId.set(account.id);
  await loadMailboxes(account.id);
  addingAccount.set(false);
  return account;
}

export async function removeAccount(id: string) {
  await apiDeleteAccount(id);
  accounts.update((a) => a.filter((acc) => acc.id !== id));
  const remaining = get(accounts);
  if (remaining.length > 0) {
    activeAccountId.set(remaining[0].id);
    await loadMailboxes(remaining[0].id);
  } else {
    activeAccountId.set(null);
    mailboxes.set([]);
    activeMailboxId.set(null);
  }
}
