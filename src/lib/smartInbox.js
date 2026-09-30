import { getSmartInboxEnabled, setSmartInboxEnabled as updateSmartInboxEnabled } from "../api.js";
import { t } from "./i18n.js";

// The backend stores built-in group names in English; show them in the
// interface language unless the user renamed them.
const BUILT_IN_NAMES = { personal: "Personal", newsletters: "Newsletters", other: "Other" };

/** `account-3-org-github` -> `org-github` */
export function categoryKind(slug) {
    return String(slug || "").replace(/^account-\d+-/, "");
}

export function categoryLabel(category) {
    const kind = categoryKind(category?.slug ?? category?.kind);
    if (BUILT_IN_NAMES[kind] && category?.name === BUILT_IN_NAMES[kind]) return t(`smart.cat.${kind}`);
    return category?.name || "";
}

let enabledState = null;

export function isSmartInboxEnabled() {
    return enabledState !== false;
}

export async function refreshSmartInboxEnabled() {
    enabledState = !!(await getSmartInboxEnabled());
    return enabledState;
}

export async function ensureSmartInboxEnabled() {
    return enabledState === null ? refreshSmartInboxEnabled() : enabledState;
}

export async function setSmartInboxEnabled(enabled) {
    await updateSmartInboxEnabled(enabled);
    enabledState = !!enabled;
    window.dispatchEvent(new CustomEvent("smart-inbox-enabled", { detail: { enabled: enabledState } }));
    return enabledState;
}
