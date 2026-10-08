import { discordClearActivity, discordSetActivity } from "../lib/api.js";
import { t } from "../lib/i18n/index.svelte.js";
import { compose } from "./compose.svelte.js";
import { mail } from "./mail.svelte.js";
import { prefs } from "./prefs.svelte.js";

export function initDiscordPresence() {
  const startedAt = Math.floor(Date.now() / 1000);

  const stop = $effect.root(() => {
    $effect(() => {
      if (!prefs.app.discordRpc) {
        discordClearActivity().catch(() => {});
        return;
      }
      const key = compose.open ? "composing" : mail.hasSelection ? "reading" : "browsing";
      discordSetActivity(t("discord.details"), t(`discord.${key}`), startedAt).catch((error) =>
        console.error("Discord presence unavailable:", error),
      );
    });
  });

  return () => {
    stop();
    discordClearActivity().catch(() => {});
  };
}
