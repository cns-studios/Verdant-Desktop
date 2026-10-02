<script>
  import { suggestContacts } from "../../lib/contacts.js";
  import { sanitizeUnicodeNoise } from "../../lib/format.js";
  import { t } from "../../lib/i18n/index.svelte.js";
  import { compose } from "../../state/compose.svelte.js";

  const BLUR_COMMIT_DELAY_MS = 80;

  let { field, label, placeholder } = $props();

  let input;
  let focused = $state(false);
  let activeIndex = $state(0);

  const recipients = $derived(compose.recipients[field]);
  const suggestions = $derived(
    focused ? suggestContacts(compose.pending[field], new Set(recipients.map((r) => r.email))) : [],
  );

  export function focus() {
    input?.focus();
  }

  const display = (contact) => {
    const name = sanitizeUnicodeNoise(contact.name || "");
    return name ? `${name} <${contact.email}>` : contact.email;
  };

  function commit() {
    const picked = suggestions[activeIndex];
    if (!picked || !compose.addRecipient(field, picked)) compose.commitPending(field);
    activeIndex = 0;
  }

  function onKeydown(event) {
    const value = compose.pending[field];
    if ((event.key === "ArrowDown" || event.key === "ArrowUp") && suggestions.length) {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      activeIndex = (activeIndex + step + suggestions.length) % suggestions.length;
    } else if (event.key === "Enter" || (event.key === "Tab" && suggestions.length)) {
      event.preventDefault();
      commit();
    } else if (event.key === "," || event.key === ";" || (event.key === " " && value.includes("@"))) {
      event.preventDefault();
      compose.commitPending(field);
    } else if (event.key === "Backspace" && !value && recipients.length) {
      compose.removeRecipient(field, recipients.length - 1);
    }
  }

  function onBlur() {
    setTimeout(() => {
      compose.commitPending(field);
      focused = false;
    }, BLUR_COMMIT_DELAY_MS);
  }
</script>

<div class="modal-field">
  <label for="compose-{field}">{label}</label>
  <div class="compose-recipient-wrap">
    <div class="compose-recipient-input">
      {#each recipients as recipient, index (recipient.email)}
        <span class="compose-recipient-chip">
          <span class="compose-recipient-chip-label" title={display(recipient)}>{display(recipient)}</span>
          <button
            class="compose-recipient-chip-remove"
            type="button"
            aria-label={t("compose.remove_recipient")}
            onclick={() => {
              compose.removeRecipient(field, index);
              input.focus();
            }}
          >
            x
          </button>
        </span>
      {/each}
      <input
        id="compose-{field}"
        type="text"
        autocomplete="off"
        {placeholder}
        bind:this={input}
        bind:value={compose.pending[field]}
        oninput={() => (activeIndex = 0)}
        onkeydown={onKeydown}
        onfocus={() => (focused = true)}
        onblur={onBlur}
      />
    </div>
    <div class={["compose-recipient-suggest", suggestions.length > 0 && "open"]}>
      {#each suggestions as suggestion, index (suggestion.email)}
        <button
          class={["compose-recipient-option", index === activeIndex && "active"]}
          type="button"
          onmousedown={(event) => {
            event.preventDefault();
            compose.addRecipient(field, suggestion);
          }}
        >
          <span class="compose-recipient-option-name">{suggestion.name || suggestion.email}</span>
          <span class="compose-recipient-option-email">{suggestion.email}</span>
        </button>
      {/each}
    </div>
  </div>
</div>
