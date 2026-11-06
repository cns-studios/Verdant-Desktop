import { discordClearActivity, discordSetActivity } from "../lib/api.js";
import { t } from "../lib/i18n/index.svelte.js";
import { compose } from "./compose.svelte.js";
import { mail } from "./mail.svelte.js";
import { prefs } from "./prefs.svelte.js";

const isVisible = () => document.visibilityState === "visible";

export function initDiscordPresence() {
  // Only time while the window is actually visible counts; time spent
  // hidden in the tray is excluded.
  let activeMs = 0;
  let shownSince = isVisible() ? Date.now() : null;
  let visible = $state(isVisible());

  const onVisibility = () => {
    if (isVisible()) {
      shownSince = Date.now();
    } else if (shownSince !== null) {
      activeMs += Date.now() - shownSince;
      shownSince = null;
    }
    visible = isVisible();
  };
  document.addEventListener("visibilitychange", onVisibility);

  const stop = $effect.root(() => {
    $effect(() => {
      if (!prefs.app.discordRpc || !visible) {
        discordClearActivity().catch(() => {});
        return;
      }
      const key = compose.open ? "composing" : mail.hasSelection ? "reading" : "browsing";
      // Pretend the session began `activeMs` before the (re)show moment.
      const startedAt = Math.floor(((shownSince ?? Date.now()) - activeMs) / 1000);
      discordSetActivity(t("discord.details"), t(`discord.${key}`), startedAt).catch((error) =>
        console.error("Discord presence unavailable:", error),
      );
    });
  });

  return () => {
    document.removeEventListener("visibilitychange", onVisibility);
    stop();
    discordClearActivity().catch(() => {});
  };
}
