import { getSmartInboxEnabled, setSmartInboxEnabled as updateSmartInboxEnabled } from "../api.js";

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
