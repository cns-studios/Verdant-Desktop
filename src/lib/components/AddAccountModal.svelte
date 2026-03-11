<script lang="ts">
  import { addingAccount, addAccount } from "../stores/accounts";
  import { showNotification, syncing } from "../stores/ui";
  import { syncAccount } from "../api";

  let name = "";
  let email = "";
  let password = "";
  let imapHost = "";
  let imapPort = 993;
  let imapTls = true;
  let smtpHost = "";
  let smtpPort = 587;
  let smtpTls = false;
  let saving = false;
  let showAdvanced = false;

  // Auto-fill common providers
  function autoFill() {
    const domain = email.split("@")[1]?.toLowerCase() ?? "";
    if (domain === "gmail.com") {
      imapHost = "imap.gmail.com"; imapPort = 993; imapTls = true;
      smtpHost = "smtp.gmail.com"; smtpPort = 587; smtpTls = false;
    } else if (domain.includes("outlook") || domain.includes("hotmail") || domain.includes("live")) {
      imapHost = "outlook.office365.com"; imapPort = 993; imapTls = true;
      smtpHost = "smtp.office365.com"; smtpPort = 587; smtpTls = false;
    } else if (domain) {
      imapHost = `imap.${domain}`; imapPort = 993; imapTls = true;
      smtpHost = `smtp.${domain}`; smtpPort = 587; smtpTls = false;
    }
    if (!name) name = email.split("@")[0] ?? "";
  }

  async function save() {
    if (!email || !password || !imapHost || !smtpHost) {
      showNotification("Please fill in all required fields.", "error");
      return;
    }
    saving = true;
    try {
      await addAccount({
        name: name || email,
        email,
        password,
        imap_host: imapHost,
        imap_port: imapPort,
        imap_tls: imapTls,
        smtp_host: smtpHost,
        smtp_port: smtpPort,
        smtp_tls: smtpTls,
      });
      showNotification(`Account ${email} added. Syncing…`);
      syncing.set(true);
      try { await syncAccount(); } finally { syncing.set(false); }
    } catch (err) {
      showNotification(`Failed to add account: ${err}`, "error");
    } finally {
      saving = false;
    }
  }

  function cancel() {
    addingAccount.set(false);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") cancel();
  }
</script>

<svelte:window on:keydown={handleKeydown} />

<div class="modal-overlay open" role="presentation">
  <div class="add-account-modal">
    <div class="modal-header">
      <span class="modal-title">Add Email Account</span>
      <button class="modal-close" on:click={cancel}>×</button>
    </div>

    <div class="modal-fields">
      <div class="modal-field">
        <label for="acc-email">Email</label>
        <input
          id="acc-email"
          type="email"
          placeholder="you@example.com"
          bind:value={email}
          on:blur={autoFill}
        />
      </div>
      <div class="modal-field">
        <label for="acc-name">Name</label>
        <input id="acc-name" type="text" placeholder="Display name" bind:value={name} />
      </div>
      <div class="modal-field">
        <label for="acc-pass">Password</label>
        <input id="acc-pass" type="password" placeholder="App password" bind:value={password} />
      </div>
    </div>

    <button class="advanced-toggle" on:click={() => showAdvanced = !showAdvanced}>
      {showAdvanced ? "▾" : "▸"} Server settings
    </button>

    {#if showAdvanced}
      <div class="modal-fields advanced">
        <div class="field-row">
          <div class="modal-field half">
            <label for="imap-host">IMAP host</label>
            <input id="imap-host" type="text" placeholder="imap.example.com" bind:value={imapHost} />
          </div>
          <div class="modal-field quarter">
            <label for="imap-port">Port</label>
            <input id="imap-port" type="number" bind:value={imapPort} />
          </div>
          <div class="modal-field check">
            <label><input type="checkbox" bind:checked={imapTls} /> TLS</label>
          </div>
        </div>
        <div class="field-row">
          <div class="modal-field half">
            <label for="smtp-host">SMTP host</label>
            <input id="smtp-host" type="text" placeholder="smtp.example.com" bind:value={smtpHost} />
          </div>
          <div class="modal-field quarter">
            <label for="smtp-port">Port</label>
            <input id="smtp-port" type="number" bind:value={smtpPort} />
          </div>
          <div class="modal-field check">
            <label><input type="checkbox" bind:checked={smtpTls} /> TLS</label>
          </div>
        </div>
      </div>
    {/if}

    <div class="modal-footer">
      <button class="cancel-btn" on:click={cancel} disabled={saving}>Cancel</button>
      <button class="send-btn" on:click={save} disabled={saving}>
        {saving ? "Connecting…" : "Add Account"}
      </button>
    </div>
  </div>
</div>

<style>
  .add-account-modal {
    width: 440px;
    background: var(--white);
    border-radius: 14px;
    box-shadow: var(--shadow-lg), 0 0 0 1px var(--border);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .advanced-toggle {
    border: none;
    background: none;
    color: var(--text-muted);
    font-size: 12px;
    cursor: pointer;
    text-align: left;
    padding: 4px 18px 8px;
    transition: color 0.12s;
  }

  .advanced-toggle:hover { color: var(--text); }

  .advanced {
    padding-top: 0;
  }

  .field-row {
    display: flex;
    gap: 8px;
    align-items: flex-end;
  }

  .half { flex: 1; }
  .quarter { width: 72px; flex-shrink: 0; }
  .check {
    padding-bottom: 11px;
    flex-shrink: 0;
    font-size: 12px;
    color: var(--text-muted);
    display: flex;
    align-items: center;
    gap: 4px;
    border-bottom: 1px solid var(--surface2);
  }

  .cancel-btn {
    padding: 8px 14px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: transparent;
    font-family: 'DM Sans', sans-serif;
    font-size: 13px;
    color: var(--text-mid);
    cursor: pointer;
    transition: background 0.12s;
  }

  .cancel-btn:hover { background: var(--surface); }
</style>
