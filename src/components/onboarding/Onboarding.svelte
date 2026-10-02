<script>
  import { untrack } from "svelte";
  import { getLang, LANGUAGES, setLang, t } from "../../lib/i18n/index.svelte.js";
  import WindowControls from "../shell/WindowControls.svelte";
  import Icon from "../ui/Icon.svelte";
  import { OnboardingFlow, STAGES } from "./flow.svelte.js";
  import GrowthPanel from "./GrowthPanel.svelte";
  import PasswordStep from "./PasswordStep.svelte";

  const STEM_REVEAL = [0.24, 0.5, 0.76, 1];

  let { onfinish, oncancel = undefined } = $props();

  const modal = $derived(!!oncancel);
  const flow = new OnboardingFlow(untrack(() => (oncancel ? "address" : "welcome")));

  function focusFirst(node) {
    node.querySelector("[autofocus]")?.focus();
  }

  function submit(handler) {
    return (event) => {
      event.preventDefault();
      handler();
    };
  }
</script>

{#snippet errorMessage()}
  {#if flow.error}
    <p class="ob-error" role="alert"><Icon name="alert-circle" /><span>{flow.error}</span></p>
  {/if}
{/snippet}

{#snippet backLink()}
  {#if !flow.busy}
    <button type="button" class="ob-link" onclick={() => flow.go("address")}>
      <Icon name="arrow-left" />{t("ob.back_to_address")}
    </button>
  {/if}
{/snippet}

<div
  class={["ob-root", modal && "ob-modal"]}
  style:--ob-stem-hidden={1 - STEM_REVEAL[flow.stage]}
  role="presentation"
  onkeydown={(event) => modal && event.key === "Escape" && !flow.busy && oncancel()}
>
  <div class="ob-frame" role="dialog" aria-modal="true" aria-labelledby="ob-title">
    <GrowthPanel stage={flow.stage} />
    <main class="ob-content">
      <div class="ob-topbar" data-tauri-drag-region={modal ? undefined : ""}>
        <div class="ob-progress" aria-hidden="true">
          {#each STAGES as name, index (name)}
            <span class={[index <= flow.stage && "on"]}></span>
          {/each}
        </div>
        <div class="ob-window-controls">
          {#if modal}
            <button class="ob-icon-btn" aria-label={t("ob.close")} onclick={oncancel}><Icon name="x" /></button>
          {:else}
            <WindowControls buttonClass="ob-icon-btn" />
          {/if}
        </div>
      </div>

      {#key flow.step}
        <div class="ob-step ob-enter" aria-live="polite" {@attach focusFirst}>
          {#if flow.step === "welcome"}
            <h1 id="ob-title">{t("ob.welcome.title")}</h1>
            <p class="ob-lede">{t("ob.welcome.body")}</p>
            <ul class="ob-facts">
              <li><Icon name="mail" /><span>{t("ob.welcome.fact_services")}</span></li>
              <li><Icon name="lock" /><span>{t("ob.welcome.fact_private")}</span></li>
            </ul>
            <div class="ob-actions">
              <button class="ob-primary" onclick={() => flow.go("address")}>{t("ob.welcome.start")}</button>
            </div>
            <label class="ob-language">
              <span>{t("ob.language")}</span>
              <select value={getLang()} onchange={(event) => setLang(event.currentTarget.value)}>
                {#each LANGUAGES as language (language.code)}
                  <option value={language.code}>{language.label}</option>
                {/each}
              </select>
            </label>
          {:else if flow.step === "address"}
            <h1 id="ob-title">{t(modal ? "ob.address.title_add" : "ob.address.title")}</h1>
            <p class="ob-lede">{t("ob.address.body")}</p>
            <form class="ob-form" novalidate onsubmit={submit(() => flow.submitAddress())}>
              <label class="ob-field">
                <span>{t("ob.address.label")}</span>
                <input
                  type="email"
                  inputmode="email"
                  autocomplete="email"
                  spellcheck="false"
                  autofocus
                  placeholder={t("ob.address.placeholder")}
                  bind:value={flow.email}
                />
              </label>
              {@render errorMessage()}
              <div class="ob-actions">
                <button class="ob-primary" type="submit">{t("ob.continue")}</button>
                <button type="button" class="ob-link" onclick={() => flow.useGoogle()}>
                  {t("ob.address.google_workspace")}
                </button>
              </div>
            </form>
          {:else if flow.step === "google"}
            <h1 id="ob-title">{t("ob.google.title")}</h1>
            <p class="ob-lede">{t("ob.google.body")}</p>
            {#if flow.busy}
              <p class="ob-status"><span class="ob-spinner"></span>{t("ob.google.waiting")}</p>
            {/if}
            {@render errorMessage()}
            <div class="ob-actions">
              <button class="ob-primary" disabled={flow.busy} onclick={() => flow.signInWithGoogle()}>
                {t(flow.error ? "ob.try_again" : "ob.google.button")}
              </button>
              {@render backLink()}
            </div>
          {:else if flow.step === "password"}
            <PasswordStep {flow} {errorMessage} {backLink} />
          {:else if flow.step === "unsupported"}
            <h1 id="ob-title">{t("ob.unsupported.title", { provider: flow.providerName })}</h1>
            <p class="ob-lede">{t(`ob.unsupported.${flow.provider.reason}`)}</p>
            <div class="ob-actions">
              <button class="ob-primary" onclick={() => flow.go("address")}>{t("ob.back_to_address")}</button>
            </div>
          {:else}
            <h1 id="ob-title">{t("ob.done.title")}</h1>
            <p class="ob-lede">{t("ob.done.body", { email: flow.connected.email })}</p>
            <div class="ob-actions">
              <button class="ob-primary" onclick={() => onfinish(flow.connected.account)}>
                {t(modal ? "ob.done.button_add" : "ob.done.button")}
              </button>
            </div>
          {/if}
        </div>
      {/key}
    </main>
  </div>
</div>
