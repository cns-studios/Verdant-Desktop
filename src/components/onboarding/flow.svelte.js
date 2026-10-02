import { addGmailAccount, addImapAccount, testImapCredentials } from "../../lib/api.js";
import { t } from "../../lib/i18n/index.svelte.js";
import { classifyConnectError, detectProvider, domainOf, isEmailAddress, usernameFor } from "../../lib/mailProviders.js";

export const STAGES = ["welcome", "address", "signin", "ready"];
const STAGE_OF_STEP = { welcome: 0, address: 1, google: 2, password: 2, unsupported: 2, done: 3 };

export class OnboardingFlow {
  step = $state("welcome");
  email = $state("");
  name = $state("");
  password = $state("");
  provider = $state(null);
  server = $state({ imapHost: "", imapPort: 993, smtpHost: "", smtpPort: 587, username: "" });
  showServer = $state(false);
  error = $state(null);
  busy = $state(false);
  connected = $state(null);

  constructor(firstStep) {
    this.step = firstStep;
  }

  get stage() {
    return STAGE_OF_STEP[this.step];
  }

  get providerName() {
    return this.provider?.name || domainOf(this.email);
  }

  get usesAppPassword() {
    return !!this.provider?.hint?.startsWith("app_password");
  }

  go(step) {
    this.error = null;
    this.busy = false;
    this.step = step;
  }

  submitAddress() {
    const email = this.email.trim();
    if (!isEmailAddress(email)) {
      this.error = t("ob.error.invalid_email");
      return;
    }
    const provider = detectProvider(email);
    this.email = email;
    this.provider = provider;
    this.password = "";
    this.showServer = false;
    this.server = {
      imapHost: provider.imapHost ?? provider.candidates?.[0]?.imapHost ?? "",
      imapPort: provider.imapPort ?? 993,
      smtpHost: provider.smtpHost ?? provider.candidates?.[0]?.smtpHost ?? "",
      smtpPort: provider.smtpPort ?? 587,
      username: usernameFor(provider, email),
    };
    this.go(provider.kind === "google" ? "google" : provider.kind === "unsupported" ? "unsupported" : "password");
  }

  useGoogle() {
    this.email = this.email.trim();
    this.provider = detectProvider("x@gmail.com");
    this.go("google");
  }

  async signInWithGoogle() {
    this.busy = true;
    this.error = null;
    try {
      this.#finish(await addGmailAccount());
    } catch (error) {
      console.error("Google sign-in failed", error);
      this.busy = false;
      this.error = t("ob.error.google");
    }
  }

  async connectWithPassword() {
    if (!this.password) {
      this.error = t("ob.error.no_password");
      return;
    }
    this.name = this.name.trim();
    this.busy = true;
    this.error = null;

    try {
      const server = await this.#resolveServer();
      const account = await addImapAccount(this.#payload(server));
      this.password = "";
      this.#finish(account);
    } catch (error) {
      console.error("Connecting account failed", error);
      this.busy = false;
      this.error = this.#describe(error);
    }
  }

  #finish(account) {
    this.connected = { email: account?.email || this.email, account };
    this.go("done");
  }

  #typedServer() {
    return {
      imapHost: this.server.imapHost.trim(),
      imapPort: parseInt(this.server.imapPort, 10) || 993,
      smtpHost: this.server.smtpHost.trim(),
      smtpPort: parseInt(this.server.smtpPort, 10) || 587,
      username: String(this.server.username).trim() || this.email,
    };
  }

  #payload(server) {
    return { email: this.email, displayName: this.name || null, password: this.password, ...server };
  }

  async #resolveServer() {
    const typed = this.#typedServer();
    if (!this.provider.unknown || this.showServer) return typed;

    let lastError = null;
    for (const candidate of this.provider.candidates) {
      const server = { ...typed, ...candidate };
      try {
        await testImapCredentials(this.#payload(server));
        this.server = server;
        return server;
      } catch (error) {
        lastError = error;
        if (classifyConnectError(error) === "auth") {
          this.server = server;
          throw error;
        }
      }
    }
    throw Object.assign(new Error(String(lastError)), { notFound: true });
  }

  #describe(error) {
    if (error?.notFound) {
      this.showServer = true;
      return t("ob.error.not_found", { domain: domainOf(this.email) });
    }
    const kind = classifyConnectError(error);
    const hint = this.provider.hint || "";
    const key = kind === "auth" && hint.startsWith("enable_imap")
      ? "ob.error.auth_enable"
      : kind === "auth" && hint.startsWith("app_password")
        ? "ob.error.auth_app"
        : `ob.error.${kind}`;
    return t(key, { provider: this.providerName, detail: String(error?.message || error) });
  }
}
