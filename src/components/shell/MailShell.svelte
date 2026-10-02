<script>
  import { openExternalUrl } from "../../lib/api.js";
  import { mailboxTitle } from "../../lib/format.js";
  import { canRunHotkey, eventCombo, isTypingTarget } from "../../lib/hotkeys.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { compose } from "../../state/compose.svelte.js";
  import { mail } from "../../state/mail.svelte.js";
  import { prefs } from "../../state/prefs.svelte.js";
  import { selection } from "../../state/selection.svelte.js";
  import { session } from "../../state/session.svelte.js";
  import { categoryLabel, smart } from "../../state/smartInbox.svelte.js";
  import { showToast, ui } from "../../state/ui.svelte.js";
  import { updates } from "../../state/updates.svelte.js";
  import ComposeModal from "../compose/ComposeModal.svelte";
  import ListPane from "../list/ListPane.svelte";
  import ReadingPane from "../reading/ReadingPane.svelte";
  import Sidebar from "../sidebar/Sidebar.svelte";
  import PaneResizer from "./PaneResizer.svelte";
  import Titlebar from "./Titlebar.svelte";

  const MAILBOX_CYCLE = ["INBOX", "STARRED", "ARCHIVE", "SENT", "DRAFT", "TRASH"];
  const DOUBLE_ESCAPE_MS = 400;

  let refreshing = false;
  let lastSelectionEscape = 0;

  const subtitle = $derived.by(() => {
    if (compose.open) return t("sidebar.compose_title");
    if (ui.settingsOpen) return t("settings.title");
    return (mail.categorySlug && categoryLabel(smart.bySlug(mail.categorySlug))) || mailboxTitle(mail.mailbox);
  });

  function dismissTopmost() {
    if (ui.contextMenu || ui.categoryPopup) {
      ui.contextMenu = null;
      ui.categoryPopup = null;
    } else if (ui.whatsNew) updates.closeWhatsNew(false);
    else if (ui.unsubscribe) ui.unsubscribe = null;
    else if (ui.smartSetup) {
      if (!ui.smartSetup.locked) ui.smartSetup = null;
    } else if (ui.addAccountOpen) return;
    else if (ui.accountPopoverOpen) ui.accountPopoverOpen = false;
    else if (ui.settingsOpen) ui.settingsOpen = false;
    else if (compose.open) compose.close();
    else if (selection.active || selection.count > 0) {
      const now = performance.now();
      if (now - lastSelectionEscape < DOUBLE_ESCAPE_MS) selection.exit();
      else selection.clear();
      lastSelectionEscape = now;
    } else mail.clearSelection();
  }

  async function refresh() {
    if (refreshing) return;
    refreshing = true;
    showToast(t("toast.fetching"));
    try {
      await mail.refresh();
    } catch (error) {
      showToast(String(error), "error");
    } finally {
      refreshing = false;
    }
  }

  function nextMailbox() {
    const index = MAILBOX_CYCLE.indexOf(mail.mailbox);
    mail.open(MAILBOX_CYCLE[(index + 1) % MAILBOX_CYCLE.length]);
  }

  function onKeydown(event) {
    const keys = prefs.hotkeys;
    const combo = eventCombo(event);
    if (combo === keys.close) return dismissTopmost();
    if (!keys.enabled) return;

    const overlayOpen = ui.settingsOpen || compose.open;
    const typing = isTypingTarget(event.target);
    const actions = [
      ["compose", combo === keys.compose, () => compose.openNew()],
      ["composeMaximize", combo === keys.composeMaximize && compose.open && !typing, () => (compose.maximized = !compose.maximized)],
      ["refresh", combo === keys.refresh, refresh],
      ["settings", combo === keys.settings, () => (ui.settingsOpen = true)],
      ["search", combo === keys.search, () => (ui.focusSearch += 1)],
      [
        "nextMailbox",
        !!keys.nextMailbox && !overlayOpen && !typing && [combo, combo.replace(/^shift\+/, "")].includes(keys.nextMailbox),
        nextMailbox,
      ],
      ["switchNextAccount", combo === keys.switchNextAccount, () => session.switchToNextAccount()],
    ];

    const match = actions.find(([, matches]) => matches);
    if (!match) return;
    event.preventDefault();
    if (canRunHotkey(match[0])) match[2]();
  }

  function openExternalLink(event) {
    const anchor = (event.target instanceof Element ? event.target : event.target?.parentElement)?.closest("a[href]");
    const href = anchor?.getAttribute("href") || "";
    if (!/^https?:\/\//.test(href)) return;
    event.preventDefault();
    event.stopPropagation();
    openExternalUrl(href).catch((error) => console.error("Failed to open link", error));
  }
</script>

<svelte:window onkeydown={onKeydown} />
<svelte:document onclickcapture={openExternalLink} />

<Titlebar {subtitle} />
<div class="app-content">
  <Sidebar />
  <ListPane />
  <PaneResizer />
  <ReadingPane />
</div>
<ComposeModal />
