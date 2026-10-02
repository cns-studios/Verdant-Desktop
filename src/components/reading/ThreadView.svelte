<script>
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { formatParticipants } from "../../lib/format.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { mail } from "../../state/mail.svelte.js";
  import ThreadBubble from "./ThreadBubble.svelte";

  const COLLAPSE_ABOVE = 2;
  const FOLD_MARGIN = 120;

  const messages = $derived(mail.conversation.messages);
  const initiallyOpen = mail.conversation.messages.length > COLLAPSE_ABOVE
    ? mail.conversation.messages.slice(-1)
    : mail.conversation.messages;
  const expanded = new SvelteSet(initiallyOpen.map((message) => message.id));

  let stack;

  function toggle(id) {
    if (!expanded.has(id)) expanded.add(id);
    else if (expanded.size > 1) expanded.delete(id);
  }

  onMount(() => {
    if (messages.length <= COLLAPSE_ABOVE) return;
    const open = stack.querySelector(".thread-bubble.expanded");
    const body = stack.parentElement;
    if (open && open.getBoundingClientRect().top > body.getBoundingClientRect().bottom - FOLD_MARGIN) {
      open.scrollIntoView({ block: "start" });
    }
  });
</script>

{#if messages.length > 1}
  <div class="thread-participant-bar">
    <span class="thread-participant-label">{formatParticipants(mail.selectedThread?.participants, 8)}</span>
    <span class="thread-message-total">{messages.length} {t("thread.messages")}</span>
  </div>
{/if}

<div class="thread-stack" bind:this={stack}>
  {#each messages as message (message.id)}
    <ThreadBubble {message} expanded={expanded.has(message.id)} ontoggle={() => toggle(message.id)} />
  {/each}
</div>
