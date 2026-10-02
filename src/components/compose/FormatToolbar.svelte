<script>
  import { escapeHtml } from "../../lib/format.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { compose } from "../../state/compose.svelte.js";
  import Icon from "../ui/Icon.svelte";

  const BLOCK_TAGS = { header: "h2", quote: "blockquote", code: "pre" };
  const FORMATS = [
    { id: "bold", icon: "bold" },
    { id: "header", icon: "h-1" },
    { id: "italic", icon: "italic" },
    { id: "list", icon: "list" },
    { id: "quote", icon: "quote" },
    { id: "code", icon: "code" },
    { id: "clear", icon: "eraser" },
  ];

  let { editor } = $props();
  let active = $state({});

  function enclosing(tag) {
    const selection = window.getSelection();
    if (!selection?.rangeCount) return null;
    const node = selection.getRangeAt(0).commonAncestorContainer;
    const element = (node.nodeType === Node.TEXT_NODE ? node.parentElement : node)?.closest?.(tag);
    return element && editor.contains(element) ? element : null;
  }

  const unwrap = (element) => element.replaceWith(document.createTextNode(element.textContent || ""));
  const toggleBlock = (tag) => document.execCommand("formatBlock", false, enclosing(tag) ? "p" : tag);

  function refresh() {
    if (!compose.formatting || !editor) return;
    active = {
      bold: document.queryCommandState("bold"),
      italic: document.queryCommandState("italic"),
      list: document.queryCommandState("insertUnorderedList"),
      header: !!enclosing("h2"),
      quote: !!enclosing("blockquote"),
      code: !!enclosing("pre"),
    };
  }

  function apply(format) {
    editor.focus();
    if (format === "bold" || format === "italic") document.execCommand(format);
    else if (format === "list") document.execCommand("insertUnorderedList");
    else if (format === "header" || format === "quote") toggleBlock(BLOCK_TAGS[format]);
    else if (format === "code") {
      const block = enclosing("pre");
      if (block) unwrap(block);
      else {
        const selected = window.getSelection()?.toString() || "code";
        document.execCommand("insertHTML", false, `<pre><code>${escapeHtml(selected)}</code></pre>`);
      }
    } else {
      document.execCommand("removeFormat");
      if (enclosing("h2") || enclosing("blockquote")) document.execCommand("formatBlock", false, "p");
      if (enclosing("ul")) document.execCommand("insertUnorderedList");
      const block = enclosing("pre");
      if (block) unwrap(block);
    }
    compose.mode = "html";
    refresh();
  }
</script>

<svelte:document onselectionchange={refresh} />

<div class={["compose-format-toolbar", compose.formatting && "open"]}>
  {#each FORMATS as format (format.id)}
    <button
      class={["compose-format-btn", active[format.id] && "active"]}
      type="button"
      title={t(`compose.format.${format.id}`)}
      onclick={() => apply(format.id)}
    >
      <Icon name={format.icon} />
    </button>
  {/each}
</div>
