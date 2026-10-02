<script>
  import { splitRecipients } from "../../lib/format.js";
  import { t } from "../../lib/i18n/index.svelte.js";

  let { email, class: className, summary = undefined } = $props();
  let expanded = $state(false);

  const to = $derived(splitRecipients(email.to_recipients));
  const cc = $derived(splitRecipients(email.cc_recipients));
  const bcc = $derived(splitRecipients(email.bcc_recipients));

  const rows = $derived(
    [
      [t("reading.from"), email.sender || ""],
      [t("compose.to"), to.join(", ")],
      [t("compose.cc"), cc.join(", ")],
      [t("compose.bcc"), bcc.join(", ")],
    ].filter(([, value]) => value),
  );

  const collapsed = $derived.by(() => {
    if (summary !== undefined) return summary;
    const all = [...to, ...cc];
    if (all.length === 1) return t("reading.to_x", { name: all[0] });
    if (all.length > 1) return t("reading.to_x_others", { name: all[0], n: all.length - 1 });
    return t((email.mailbox || "").toUpperCase() === "SENT" ? "reading.recipients_loading" : "reading.to_me");
  });

  function toggle(event) {
    event.stopPropagation();
    expanded = !expanded;
  }
</script>

<div
  class={className}
  role="button"
  tabindex="0"
  title={t("reading.expand_recipients")}
  style:cursor="pointer"
  onclick={toggle}
  onkeydown={(event) => event.key === "Enter" && toggle(event)}
>
  {#if expanded && rows.length}
    {#each rows as [label, value] (label)}
      <div class="rcpt-row"><span class="rcpt-label">{label}</span><span class="rcpt-value">{value}</span></div>
    {/each}
  {:else}
    {collapsed}
  {/if}
</div>
