<script>
  import { escapeHtml } from "../../lib/format.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { updates } from "../../state/updates.svelte.js";
  import Icon from "../ui/Icon.svelte";

  const CONFETTI_COUNT = 50;
  const CONFETTI_COLORS = [
    "#5aa876", "#4CAF50", "#FF6B6B", "#FFD93D", "#6BCB77", "#4D96FF", "#FF8C42", "#A78BFA",
    "#F472B6", "#10B981", "#EC4899", "#F59E0B", "#8B5CF6", "#06B6D4", "#EF4444", "#14B8A6",
  ];

  let { version, content } = $props();

  const confetti = Array.from({ length: CONFETTI_COUNT }, () => {
    const size = Math.random() * 8 + 6;
    return [
      `left:${Math.random() * 100}%`,
      `top:${Math.random() * -20 - 10}px`,
      `width:${size}px`,
      `height:${size}px`,
      `background:${CONFETTI_COLORS[Math.floor(Math.random() * CONFETTI_COLORS.length)]}`,
      `border-radius:${Math.random() > 0.5 ? "50%" : "0"}`,
      `animation:confetti-fall ${Math.random() * 2 + 2.5}s linear ${Math.random() * 0.2}s forwards`,
      `transform:rotate(${Math.random() * 360}deg)`,
    ].join(";");
  });

  const inline = (text) =>
    escapeHtml(text)
      .replace(/\*\*(.+?)\*\*|__(.+?)__/g, "<strong>$1$2</strong>")
      .replace(/\*(.+?)\*|_(.+?)_/g, "<em>$1$2</em>")
      .replace(/`(.+?)`/g, "<code>$1</code>");

  const blocks = $derived.by(() => {
    const result = [];
    for (const line of (content || "").split("\n").map((l) => l.trim())) {
      if (line.startsWith("> ")) {
        if (!result.at(-1)?.quote) result.push({ quote: [] });
        result.at(-1).quote.push(inline(line.slice(2).trim()));
      } else if (line.startsWith("- ")) {
        result.push({ item: inline(line.slice(2).trim()) });
      }
    }
    return result;
  });
</script>

<div class="whatsnew-confetti-container">
  {#each confetti as style, index (index)}
    <div class="confetti" {style}></div>
  {/each}
</div>

<div class="whatsnew-backdrop" role="presentation" onclick={() => updates.closeWhatsNew(false)}></div>
<div class="whatsnew-modal" role="dialog" aria-modal="true" aria-label={t("whatsnew.title")}>
  <div class="whatsnew-header">
    <h2 class="whatsnew-title">{t("whatsnew.title")}</h2>
    <button class="whatsnew-close-btn" aria-label={t("reading.close")} onclick={() => updates.closeWhatsNew(false)}>
      <Icon name="x" />
    </button>
  </div>
  <div class="whatsnew-content">
    <div class="whatsnew-version">v{version}</div>
    {#if blocks.length}
      <ul class="whatsnew-changes">
        {#each blocks as block, index (index)}
          {#if block.quote}
            <blockquote class="whatsnew-blockquote">
              {#each block.quote as paragraph, i (i)}
                <p>{@html paragraph}</p>
              {/each}
            </blockquote>
          {:else}
            <li>{@html block.item}</li>
          {/if}
        {/each}
      </ul>
    {:else}
      <div class="whatsnew-empty">{t("whatsnew.no_changes")}</div>
    {/if}
  </div>
  <div class="whatsnew-footer">
    <button class="whatsnew-dismiss-btn" onclick={() => updates.closeWhatsNew(true)}>{t("whatsnew.dismiss")}</button>
  </div>
</div>
