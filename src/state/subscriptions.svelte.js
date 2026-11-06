import { SvelteSet } from "svelte/reactivity";
import * as api from "../lib/api.js";
import { t } from "../lib/i18n/index.svelte.js";
import { startProgress } from "./ui.svelte.js";

const CONCURRENCY = 3;

class SubscriptionsStore {
  items = $state([]);
  loading = $state(false);
  loaded = $state(false);
  error = $state("");
  query = $state("");
  selected = new SvelteSet();
  status = $state({});
  expanded = new SvelteSet();
  allMessages = $state({});

  #loadId = 0;

  active = $derived(this.items.filter((item) => !item.unsubscribed));
  done = $derived(this.items.filter((item) => item.unsubscribed));

  matches(item) {
    const query = this.query.trim().toLowerCase();
    return !query || `${item.name} ${item.address}`.toLowerCase().includes(query);
  }

  visibleActive = $derived(this.active.filter((item) => this.matches(item)));
  visibleDone = $derived(this.done.filter((item) => this.matches(item)));

  busy = $derived(Object.values(this.status).some((state) => state === "busy"));

  async load() {
    const id = ++this.#loadId;
    this.loading = !this.loaded;
    try {
      const items = await api.getSubscriptions();
      if (id !== this.#loadId) return;
      this.items = items;
      this.allMessages = {};
      this.error = "";
      const live = new Set(items.filter((item) => !item.unsubscribed).map((item) => item.sender_key));
      for (const key of [...this.selected]) if (!live.has(key)) this.selected.delete(key);
      this.loaded = true;
    } catch (error) {
      if (id === this.#loadId) this.error = String(error);
    } finally {
      if (id === this.#loadId) this.loading = false;
    }
  }

  toggle(key) {
    if (this.selected.has(key)) this.selected.delete(key);
    else this.selected.add(key);
  }

  toggleExpanded(key) {
    if (this.expanded.has(key)) this.expanded.delete(key);
    else this.expanded.add(key);
  }

  selectAllVisible() {
    this.visibleActive.forEach((item) => this.selected.add(item.sender_key));
  }

  async showAll(key) {
    if (this.allMessages[key]) return;
    this.allMessages[key] = "loading";
    try {
      this.allMessages[key] = await api.getSenderEmails(key);
    } catch (error) {
      console.error(`Failed to load emails for ${key}`, error);
      delete this.allMessages[key];
    }
  }

  clearSelection() {
    this.selected.clear();
  }

  async unsubscribe(keys) {
    const queue = keys.filter((key) => this.status[key] !== "busy");
    queue.forEach((key) => (this.status[key] = "busy"));
    const results = { ok: 0, failed: 0 };
    const tracker = startProgress(t("progress.unsubscribing"), queue.length);

    const worker = async () => {
      while (queue.length) {
        const key = queue.shift();
        try {
          await api.unsubscribeSender(key);
          const item = this.items.find((entry) => entry.sender_key === key);
          if (item) item.unsubscribed = true;
          this.selected.delete(key);
          delete this.status[key];
          results.ok += 1;
        } catch (error) {
          console.error(`Unsubscribe failed for ${key}`, error);
          this.status[key] = "error";
          results.failed += 1;
        }
        tracker?.tick();
      }
    };
    try {
      await Promise.all(Array.from({ length: Math.min(CONCURRENCY, queue.length) }, worker));
    } finally {
      tracker?.finish();
    }
    return results;
  }
}

export const subscriptions = new SubscriptionsStore();

export function resetSubscriptions() {
  subscriptions.items = [];
  subscriptions.loaded = false;
  subscriptions.selected.clear();
  subscriptions.expanded.clear();
  subscriptions.status = {};
  subscriptions.allMessages = {};
  subscriptions.query = "";
}
