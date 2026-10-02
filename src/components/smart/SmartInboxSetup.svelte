<script>
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";
  import * as api from "../../lib/api.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { mail } from "../../state/mail.svelte.js";
  import { categoryLabel, smart } from "../../state/smartInbox.svelte.js";
  import { ui } from "../../state/ui.svelte.js";
  import Icon from "../ui/Icon.svelte";

  const PROGRESS_POLL_MS = 150;

  let { resort } = $props();

  let phase = $state("loading");
  let groups = $state([]);
  let progress = $state(null);
  let failed = $state(false);
  let settled = $state(false);
  let dialog;
  let aborted = false;

  const total = $derived(groups.reduce((sum, group) => sum + (group.message_count || 0), 0));
  const largest = $derived(Math.max(1, ...groups.map((group) => group.message_count || 0)));
  const empty = $derived(phase === "preview" && total === 0);

  const title = $derived(
    phase === "done" ? t("smart.setup.done_title") : t(resort && !empty ? "smart.setup.title_again" : "smart.setup.title"),
  );
  const lede = $derived.by(() => {
    if (phase === "loading") return t("smart.setup.loading");
    if (phase === "done") return t("smart.setup.done_body");
    return t(empty ? "smart.setup.empty" : "smart.setup.body");
  });
  const note = $derived(
    phase !== "loading" && phase !== "done" && !empty ? t(resort ? "smart.setup.note_again" : "smart.setup.note") : "",
  );

  onMount(showPreview);

  function close() {
    if (phase !== "sorting") ui.smartSetup = null;
  }

  async function showPreview() {
    progress = null;
    try {
      groups = (await api.previewInboxCategories()) || [];
    } catch (error) {
      console.error("Smart inbox preview failed", error);
      groups = [];
      failed = true;
    }
    phase = "preview";
    dialog?.focus();
  }

  $effect(() => {
    ui.smartSetup.locked = phase === "sorting";
  });

  async function sort() {
    phase = "sorting";
    aborted = false;
    failed = false;
    const poll = setInterval(async () => {
      const state = await api.getCategorizeProgress().catch(() => null);
      if (state?.total) progress = Math.min(1, state.processed / state.total);
    }, PROGRESS_POLL_MS);

    try {
      await api.categorizeInbox();
      clearInterval(poll);
      if (aborted) return showPreview();

      await smart.setEnabled(true);
      groups = smart.categories;
      progress = null;
      phase = "done";
      requestAnimationFrame(() => requestAnimationFrame(() => (settled = true)));
      await mail.reload();
    } catch (error) {
      clearInterval(poll);
      console.error("Smart inbox sorting failed", error);
      await showPreview();
      failed = true;
    }
  }

  async function stop() {
    aborted = true;
    await api.abortCategorizeInbox();
  }
</script>

<div
  class={["sis-overlay", "open", phase === "sorting" && "is-sorting", settled && "is-sorted"]}
  role="presentation"
  transition:fade={{ duration: 180 }}
  onclick={(event) => event.target === event.currentTarget && close()}
>
  <div class="sis-dialog" role="dialog" tabindex="-1" aria-modal="true" aria-labelledby="sis-title" bind:this={dialog}>
    <button class="sis-close" aria-label={t("reading.close")} onclick={close}><Icon name="x" /></button>
    <h2 id="sis-title">{title}</h2>
    <p class="sis-lede">{lede}</p>
    <ul class="sis-groups" aria-live="polite" style:--progress={progress}>
      {#if !empty}
        {#each groups as group, index (group.slug ?? group.kind)}
          <li
            class="sis-group"
            style:--group-color={group.color || "#7b8075"}
            style:--share={(group.message_count || 0) / largest}
            style:--i={index}
          >
            <span class="sis-dot"></span>
            <span class="sis-name">{categoryLabel(group)}</span>
            <span class="sis-bar"><i></i></span>
            <span class="sis-count">{t("smart.setup.count", { n: group.message_count || 0 })}</span>
          </li>
        {/each}
      {/if}
    </ul>
    {#if note}
      <p class="sis-note">{note}</p>
    {/if}
    {#if failed}
      <p class="sis-error" role="alert"><Icon name="alert-circle" /><span>{t("smart.setup.failed")}</span></p>
    {/if}
    <div class="sis-actions">
      {#if phase === "done"}
        <button class="sis-primary" onclick={close}>{t("smart.setup.done")}</button>
      {:else}
        <button class="sis-primary" disabled={phase !== "preview"} onclick={sort}>
          {#if phase === "sorting"}
            <span class="sis-spinner"></span>{t("smart.setup.sorting")}
          {:else}
            {t(empty ? "smart.setup.turn_on" : resort ? "smart.setup.sort_again" : "smart.setup.sort")}
          {/if}
        </button>
        <button class="sis-secondary" onclick={phase === "sorting" ? stop : close}>
          {t(phase === "sorting" ? "smart.setup.stop" : "smart.setup.not_now")}
        </button>
      {/if}
    </div>
  </div>
</div>
