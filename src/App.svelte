<script>
  import { onMount } from "svelte";
  import Onboarding from "./components/onboarding/Onboarding.svelte";
  import Overlays from "./components/overlays/Overlays.svelte";
  import MailShell from "./components/shell/MailShell.svelte";
  import StatusScreen from "./components/shell/StatusScreen.svelte";
  import { getLang, t } from "./lib/i18n/index.svelte.js";
  import { mail } from "./state/mail.svelte.js";
  import { prefs } from "./state/prefs.svelte.js";
  import { session } from "./state/session.svelte.js";

  onMount(() => session.boot());

  $effect(() => {
    document.documentElement.lang = getLang();
    document.documentElement.classList.toggle("dark", !!prefs.app.useDarkMode);
  });

  $effect(() => {
    document.body.dataset.textSize = prefs.app.textSize;
    document.body.classList.toggle("sidebar-collapsed", prefs.sidebarCollapsed);
    document.body.classList.toggle("reading-pane-hidden", !mail.hasSelection);
  });
</script>

{#if session.status === "booting"}
  <div class="boot-screen"><div class="boot-spinner"></div></div>
{:else if session.status === "config-missing"}
  <StatusScreen title={t("app.config_required")} detail={t("app.config_missing")} />
{:else if session.status === "error"}
  <StatusScreen
    title={t("app.init_failed")}
    detail={session.error}
    action={t("app.retry")}
    onaction={() => session.boot()}
  />
{:else if session.status === "onboarding"}
  <Onboarding onfinish={(account) => session.accountAdded(account)} />
{:else}
  <MailShell />
{/if}

<Overlays />
