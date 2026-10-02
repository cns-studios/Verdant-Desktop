<script>
  import { avatarUrl, cachedAvatar, loadAvatar, senderInitials } from "../../lib/avatars.js";

  let { sender = "", mailbox = "", class: className = "sender-avatar" } = $props();

  const url = $derived(avatarUrl(sender, mailbox));
  let image = $state(null);

  $effect(() => {
    const current = url;
    image = current ? cachedAvatar(current) : null;
    if (!current || image) return;

    let cancelled = false;
    loadAvatar(current).then((dataUrl) => {
      if (!cancelled) image = dataUrl;
    });
    return () => (cancelled = true);
  });
</script>

<div class={[className, image && "has-image"]}>
  {#if image}
    <img alt="" src={image} />
  {:else}
    {senderInitials(sender)}
  {/if}
</div>
