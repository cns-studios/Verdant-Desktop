import * as api from "../lib/api.js";
import { t } from "../lib/i18n/index.svelte.js";
import { readText, writeText } from "../lib/storage.js";
import { closeFloatingMenus } from "./ui.svelte.js";

const EXPANDED_KEY = "verdant.smartInboxExpanded";
const BUILT_IN_NAMES = { personal: "Personal", newsletters: "Newsletters", other: "Other" };
const REFRESH_DELAY_MS = 250;

export function categoryLabel(category) {
  const kind = String(category?.slug ?? category?.kind ?? "").replace(/^account-\d+-/, "");
  if (BUILT_IN_NAMES[kind] && category?.name === BUILT_IN_NAMES[kind]) return t(`smart.cat.${kind}`);
  return category?.name || "";
}

class SmartInbox {
  enabled = $state(true);
  categories = $state([]);
  expanded = $state(readText(EXPANDED_KEY) !== "0");
  #refreshTimer = null;

  get sorted() {
    return this.enabled && this.categories.length > 0;
  }

  bySlug(slug) {
    return this.categories.find((c) => c.slug === slug) ?? null;
  }

  setExpanded(expanded) {
    this.expanded = expanded;
    writeText(EXPANDED_KEY, expanded ? "1" : "0");
  }

  async refresh() {
    try {
      this.enabled = !!(await api.getSmartInboxEnabled());
      this.categories = this.enabled ? await api.getInboxCategories() : [];
    } catch (error) {
      console.error("Failed to load smart inbox", error);
    }
  }

  refreshSoon() {
    clearTimeout(this.#refreshTimer);
    this.#refreshTimer = setTimeout(() => this.refresh(), REFRESH_DELAY_MS);
  }

  async setEnabled(enabled) {
    await api.setSmartInboxEnabled(enabled);
    this.enabled = enabled;
    if (!enabled) {
      this.categories = [];
      closeFloatingMenus();
    }
    await this.refresh();
  }

  async rename(slug, name) {
    const next = name.trim();
    const category = this.bySlug(slug);
    if (!category || !next || next === categoryLabel(category)) return;
    try {
      await api.renameInboxCategory(slug, next);
    } catch (error) {
      console.error("Failed to rename smart inbox category", error);
    }
    await this.refresh();
  }
}

export const smart = new SmartInbox();
