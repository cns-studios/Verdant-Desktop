<script>
  import { t } from "../../lib/i18n/index.svelte.js";
  import Icon from "../ui/Icon.svelte";

  let { flow, errorMessage, backLink } = $props();

  let revealed = $state(false);
  let passwordInput;

  const provider = $derived(flow.provider);

  function connect(event) {
    event.preventDefault();
    flow.connectWithPassword();
  }

  function toggleReveal() {
    revealed = !revealed;
    passwordInput.focus();
  }
</script>

<h1 id="ob-title">
  {provider.unknown ? t("ob.password.title_generic") : t("ob.password.title", { provider: flow.providerName })}
</h1>
<p class="ob-lede">
  {t(flow.usesAppPassword ? "ob.password.body_app" : "ob.password.body", { provider: flow.providerName, email: flow.email })}
</p>
{#if provider.hint}
  <div class="ob-hint"><Icon name="info-circle" /><p>{t(`ob.hint.${provider.hint}`)}</p></div>
{/if}

<form class="ob-form" novalidate onsubmit={connect}>
  <label class="ob-field">
    <span>{t(flow.usesAppPassword ? "ob.password.label_app" : "ob.password.label")}</span>
    <span class="ob-password">
      <input
        type={revealed ? "text" : "password"}
        autocomplete="current-password"
        autofocus
        disabled={flow.busy}
        bind:this={passwordInput}
        bind:value={flow.password}
      />
      <button type="button" class="ob-reveal" aria-pressed={revealed} onclick={toggleReveal}>
        {t(revealed ? "ob.password.hide" : "ob.password.show")}
      </button>
    </span>
  </label>
  <label class="ob-field">
    <span>{t("ob.password.name_label")} <em>{t("ob.optional")}</em></span>
    <input type="text" autocomplete="name" disabled={flow.busy} bind:value={flow.name} />
  </label>

  <details class="ob-server" bind:open={flow.showServer}>
    <summary>{t("ob.server.toggle")}</summary>
    <p class="ob-server-note">{t("ob.server.note")}</p>
    <div class="ob-server-grid">
      <label class="ob-field">
        <span>{t("ob.server.incoming")}</span>
        <input type="text" spellcheck="false" bind:value={flow.server.imapHost} />
      </label>
      <label class="ob-field ob-port">
        <span>{t("ob.server.port")}</span>
        <input type="number" inputmode="numeric" bind:value={flow.server.imapPort} />
      </label>
      <label class="ob-field">
        <span>{t("ob.server.outgoing")}</span>
        <input type="text" spellcheck="false" bind:value={flow.server.smtpHost} />
      </label>
      <label class="ob-field ob-port">
        <span>{t("ob.server.port")}</span>
        <input type="number" inputmode="numeric" bind:value={flow.server.smtpPort} />
      </label>
      <label class="ob-field ob-wide">
        <span>{t("ob.server.username")}</span>
        <input type="text" spellcheck="false" bind:value={flow.server.username} />
      </label>
    </div>
  </details>

  {#if flow.busy}
    <p class="ob-status"><span class="ob-spinner"></span>{t("ob.password.connecting", { provider: flow.providerName })}</p>
  {/if}
  {@render errorMessage()}
  <div class="ob-actions">
    <button class="ob-primary" type="submit" disabled={flow.busy}>{t("ob.password.connect")}</button>
    {@render backLink()}
  </div>
</form>
