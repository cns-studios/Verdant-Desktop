import { writable, get } from "svelte/store";
import type { Message } from "../api";
import {
  listMessages,
  fetchMessageBody,
  markRead,
  searchMessages as apiSearch,
  isUnread,
} from "../api";

export const messages = writable<Message[]>([]);
export const activeMessageId = writable<string | null>(null);
export const activeMessage = writable<Message | null>(null);
export const loadingMessages = writable(false);
export const loadingBody = writable(false);
export const searchQuery = writable("");
export const searchResults = writable<Message[] | null>(null);

export const displayedMessages = {
  subscribe: (run: (value: Message[]) => void) => {
    // derived manually to avoid circular imports
    let msgs: Message[] = [];
    let results: Message[] | null = null;
    const unsub1 = messages.subscribe((v) => { msgs = v; run(results ?? msgs); });
    const unsub2 = searchResults.subscribe((v) => { results = v; run(results ?? msgs); });
    return () => { unsub1(); unsub2(); };
  }
};

export async function loadMessages(mailboxId: string) {
  loadingMessages.set(true);
  try {
    const list = await listMessages(mailboxId);
    messages.set(list);
    searchResults.set(null);
    // Restore selected message if still in list
    const current = get(activeMessageId);
    if (current && !list.find((m) => m.id === current)) {
      activeMessageId.set(null);
      activeMessage.set(null);
    }
  } finally {
    loadingMessages.set(false);
  }
}

export async function selectMessage(msg: Message) {
  activeMessageId.set(msg.id);

  // Optimistically show with what we have
  activeMessage.set(msg);

  if (!msg.body_fetched) {
    loadingBody.set(true);
    try {
      const full = await fetchMessageBody(msg.id);
      activeMessage.set(full);
      // Update in list too
      messages.update((list) =>
        list.map((m) => (m.id === full.id ? full : m))
      );
    } finally {
      loadingBody.set(false);
    }
  }

  // Mark as read locally (IMAP flag sync happens in background)
  if (isUnread(msg)) {
    await markRead(msg.id);
    messages.update((list) =>
      list.map((m) => {
        if (m.id === msg.id) {
          const flags = JSON.parse(m.flags || "[]") as string[];
          if (!flags.includes("\\Seen")) flags.push("\\Seen");
          return { ...m, flags: JSON.stringify(flags) };
        }
        return m;
      })
    );
  }
}

export async function searchMessages(query: string, accountId?: string) {
  if (!query.trim()) {
    searchResults.set(null);
    return;
  }
  const results = await apiSearch(query, accountId);
  searchResults.set(results);
}
