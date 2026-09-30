import { addGmailAccount, addImapAccount, testImapCredentials } from "../api.js";
import { escapeHtml } from "../lib/format.js";
import { t, getLang, setLang, getSupportedLanguages, initLang } from "../lib/i18n.js";
import {
    isEmailAddress, domainOf, detectProvider, usernameFor, classifyConnectError,
} from "../lib/mailProviders.js";
import { icon } from "./icons.js";

// One question per screen. The stem on the left grows a leaf per stage and
// doubles as the progress indicator.
const STAGES = ["welcome", "address", "signin", "ready"];
const STAGE_OF_STEP = { welcome: 0, address: 1, google: 2, password: 2, unsupported: 2, done: 3 };
// Share of the stem drawn at each stage (it reaches the top when ready).
const STEM_REVEAL = [0.24, 0.5, 0.76, 1];
const LEAVES = [
    { x: 60, y: 318, angle: 200 },
    { x: 57, y: 236, angle: -20 },
    { x: 61, y: 150, angle: 205 },
    { x: 60, y: 62, angle: -35 },
];

function stemSvg() {
    const leaf = "M0 0 C 10 -13, 32 -15, 46 -2 C 33 11, 12 12, 0 0 Z";
    return `
        <svg class="ob-stem" viewBox="30 0 60 400" preserveAspectRatio="xMidYMid meet" aria-hidden="true">
            <path class="ob-stem-line" pathLength="1"
                d="M60 396 C 60 350, 54 320, 58 280 S 64 200, 58 160 S 56 90, 60 50" />
            ${LEAVES.map((l, i) => `
                <g transform="translate(${l.x} ${l.y}) rotate(${l.angle})">
                    <path class="ob-leaf" data-leaf="${i}" d="${leaf}" />
                </g>`).join("")}
        </svg>`;
}

function stageListHtml() {
    return `<ol class="ob-stages">${STAGES.map((s, i) => `
        <li data-stage="${i}">${escapeHtml(t(`ob.stage.${s}`))}</li>`).join("")}
    </ol>`;
}

export function showOnboarding(onSuccess, cancelable = false) {
    initLang();
    document.getElementById("verdant-onboarding")?.remove();

    const root = document.createElement("div");
    root.id = "verdant-onboarding";
    root.className = `ob-root${cancelable ? " ob-modal" : ""}`;

    const state = {
        step: cancelable ? "address" : "welcome",
        email: "",
        name: "",
        provider: null,
        error: null,
        busy: false,
        showServer: false,
        server: null,
        connectedEmail: "",
        connectedAccount: null,
    };
    // Kept out of `state` and never written into markup; re-applied to the
    // field after each re-render so a typo can be fixed without retyping.
    let secretDraft = "";

    root.innerHTML = `
        <div class="ob-frame" role="dialog" aria-modal="true" aria-labelledby="ob-title">
            <aside class="ob-panel">
                <div class="ob-wordmark">Verdant</div>
                <div class="ob-growth">${stemSvg()}${stageListHtml()}</div>
            </aside>
            <main class="ob-content">
                <div class="ob-topbar" ${cancelable ? "" : "data-tauri-drag-region"}>
                    <div class="ob-progress" aria-hidden="true">${STAGES.map(() => "<span></span>").join("")}</div>
                    <div class="ob-window-controls"></div>
                </div>
                <div class="ob-step" aria-live="polite"></div>
            </main>
        </div>`;

    const controls = root.querySelector(".ob-window-controls");
    if (cancelable) {
        controls.innerHTML = `<button class="ob-icon-btn" data-action="dismiss" aria-label="${escapeHtml(t("ob.close"))}">${icon("x")}</button>`;
        controls.querySelector("button").onclick = () => root.remove();
        root.addEventListener("keydown", e => { if (e.key === "Escape" && !state.busy) root.remove(); });
    } else {
        controls.innerHTML = `
            <button class="ob-icon-btn" id="ob-min-btn" aria-label="${escapeHtml(t("ob.window.minimize"))}">${icon("minus")}</button>
            <button class="ob-icon-btn" id="ob-max-btn" aria-label="${escapeHtml(t("ob.window.maximize"))}">${icon("square")}</button>
            <button class="ob-icon-btn" id="ob-close-btn" aria-label="${escapeHtml(t("ob.window.close"))}">${icon("x")}</button>`;
        import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
            const win = getCurrentWindow();
            controls.querySelector("#ob-min-btn").onclick = () => win.minimize();
            controls.querySelector("#ob-max-btn").onclick = () => win.toggleMaximize();
            controls.querySelector("#ob-close-btn").onclick = () => win.close();
        }).catch(() => {});
    }

    (cancelable ? document.body : document.getElementById("root")).appendChild(root);

    const go = (step, patch = {}) => {
        Object.assign(state, { error: null, busy: false }, patch, { step });
        render();
    };

    function setStage() {
        const stage = STAGE_OF_STEP[state.step];
        root.style.setProperty("--ob-stem-hidden", String(1 - STEM_REVEAL[stage]));
        root.querySelectorAll(".ob-leaf").forEach(leaf => {
            leaf.classList.toggle("grown", Number(leaf.dataset.leaf) <= stage);
        });
        root.querySelectorAll(".ob-stages li").forEach(li => {
            const i = Number(li.dataset.stage);
            li.classList.toggle("done", i < stage);
            li.classList.toggle("current", i === stage);
            if (i === stage) li.setAttribute("aria-current", "step");
            else li.removeAttribute("aria-current");
        });
        root.querySelectorAll(".ob-progress span").forEach((bar, i) => bar.classList.toggle("on", i <= stage));
    }

    function render() {
        setStage();
        const step = root.querySelector(".ob-step");
        step.innerHTML = VIEWS[state.step]();
        step.classList.remove("ob-enter");
        void step.offsetWidth;
        step.classList.add("ob-enter");
        BINDERS[state.step]?.(step);
        step.querySelector("[autofocus]")?.focus();
        step.querySelector(".ob-error")?.scrollIntoView({ block: "nearest" });
    }

    const providerName = () => state.provider?.name || domainOf(state.email);

    const errorHtml = () => state.error
        ? `<p class="ob-error" role="alert">${icon("alert-circle")}<span>${escapeHtml(state.error)}</span></p>`
        : "";

    const backLink = () => state.busy ? "" :
        `<button type="button" class="ob-link" data-action="back">${icon("arrow-left")}${escapeHtml(t("ob.back_to_address"))}</button>`;

    const VIEWS = {
        welcome: () => `
            <h1 id="ob-title">${escapeHtml(t("ob.welcome.title"))}</h1>
            <p class="ob-lede">${escapeHtml(t("ob.welcome.body"))}</p>
            <ul class="ob-facts">
                <li>${icon("mail")}<span>${escapeHtml(t("ob.welcome.fact_services"))}</span></li>
                <li>${icon("lock")}<span>${escapeHtml(t("ob.welcome.fact_private"))}</span></li>
            </ul>
            <div class="ob-actions">
                <button class="ob-primary" data-action="start">${escapeHtml(t("ob.welcome.start"))}</button>
            </div>
            <label class="ob-language">
                <span>${escapeHtml(t("ob.language"))}</span>
                <select id="ob-lang-select">
                    ${getSupportedLanguages().map(l => `<option value="${l.code}" ${l.code === getLang() ? "selected" : ""}>${escapeHtml(l.label)}</option>`).join("")}
                </select>
            </label>`,

        address: () => `
            <h1 id="ob-title">${escapeHtml(t(cancelable ? "ob.address.title_add" : "ob.address.title"))}</h1>
            <p class="ob-lede">${escapeHtml(t("ob.address.body"))}</p>
            <form class="ob-form" novalidate>
                <label class="ob-field">
                    <span>${escapeHtml(t("ob.address.label"))}</span>
                    <input id="ob-email" type="email" inputmode="email" autocomplete="email" spellcheck="false"
                        placeholder="${escapeHtml(t("ob.address.placeholder"))}" value="${escapeHtml(state.email)}" autofocus>
                </label>
                ${errorHtml()}
                <div class="ob-actions">
                    <button class="ob-primary" type="submit">${escapeHtml(t("ob.continue"))}</button>
                    <button type="button" class="ob-link" data-action="google">${escapeHtml(t("ob.address.google_workspace"))}</button>
                </div>
            </form>`,

        google: () => `
            <h1 id="ob-title">${escapeHtml(t("ob.google.title"))}</h1>
            <p class="ob-lede">${escapeHtml(t("ob.google.body"))}</p>
            ${state.busy ? `<p class="ob-status"><span class="ob-spinner"></span>${escapeHtml(t("ob.google.waiting"))}</p>` : ""}
            ${errorHtml()}
            <div class="ob-actions">
                <button class="ob-primary" data-action="google-signin" ${state.busy ? "disabled" : ""}>
                    ${escapeHtml(t(state.error ? "ob.try_again" : "ob.google.button"))}
                </button>
                ${backLink()}
            </div>`,

        password: () => {
            const p = state.provider;
            const server = state.server || {};
            return `
            <h1 id="ob-title">${escapeHtml(p.unknown ? t("ob.password.title_generic") : t("ob.password.title", { provider: providerName() }))}</h1>
            <p class="ob-lede">${escapeHtml(t(p.hint?.startsWith("app_password") ? "ob.password.body_app" : "ob.password.body", { provider: providerName(), email: state.email }))}</p>
            ${p.hint ? `<div class="ob-hint">${icon("info-circle")}<p>${escapeHtml(t(`ob.hint.${p.hint}`))}</p></div>` : ""}
            <form class="ob-form" novalidate>
                <label class="ob-field">
                    <span>${escapeHtml(t(p.hint?.startsWith("app_password") ? "ob.password.label_app" : "ob.password.label"))}</span>
                    <span class="ob-password">
                        <input id="ob-password" type="password" autocomplete="current-password" autofocus ${state.busy ? "disabled" : ""}>
                        <button type="button" class="ob-reveal" data-action="reveal" aria-pressed="false">${escapeHtml(t("ob.password.show"))}</button>
                    </span>
                </label>
                <label class="ob-field">
                    <span>${escapeHtml(t("ob.password.name_label"))} <em>${escapeHtml(t("ob.optional"))}</em></span>
                    <input id="ob-name" type="text" autocomplete="name" value="${escapeHtml(state.name)}" ${state.busy ? "disabled" : ""}>
                </label>
                <details class="ob-server" ${state.showServer ? "open" : ""}>
                    <summary>${escapeHtml(t("ob.server.toggle"))}</summary>
                    <p class="ob-server-note">${escapeHtml(t("ob.server.note"))}</p>
                    <div class="ob-server-grid">
                        <label class="ob-field"><span>${escapeHtml(t("ob.server.incoming"))}</span>
                            <input id="ob-imap-host" type="text" spellcheck="false" value="${escapeHtml(server.imapHost ?? p.imapHost ?? p.candidates?.[0]?.imapHost ?? "")}"></label>
                        <label class="ob-field ob-port"><span>${escapeHtml(t("ob.server.port"))}</span>
                            <input id="ob-imap-port" type="number" inputmode="numeric" value="${escapeHtml(String(server.imapPort ?? p.imapPort ?? 993))}"></label>
                        <label class="ob-field"><span>${escapeHtml(t("ob.server.outgoing"))}</span>
                            <input id="ob-smtp-host" type="text" spellcheck="false" value="${escapeHtml(server.smtpHost ?? p.smtpHost ?? p.candidates?.[0]?.smtpHost ?? "")}"></label>
                        <label class="ob-field ob-port"><span>${escapeHtml(t("ob.server.port"))}</span>
                            <input id="ob-smtp-port" type="number" inputmode="numeric" value="${escapeHtml(String(server.smtpPort ?? p.smtpPort ?? 587))}"></label>
                        <label class="ob-field ob-wide"><span>${escapeHtml(t("ob.server.username"))}</span>
                            <input id="ob-username" type="text" spellcheck="false" value="${escapeHtml(server.username ?? usernameFor(p, state.email))}"></label>
                    </div>
                </details>
                ${state.busy ? `<p class="ob-status"><span class="ob-spinner"></span>${escapeHtml(t("ob.password.connecting", { provider: providerName() }))}</p>` : ""}
                ${errorHtml()}
                <div class="ob-actions">
                    <button class="ob-primary" type="submit" ${state.busy ? "disabled" : ""}>${escapeHtml(t("ob.password.connect"))}</button>
                    ${backLink()}
                </div>
            </form>`;
        },

        unsupported: () => `
            <h1 id="ob-title">${escapeHtml(t("ob.unsupported.title", { provider: providerName() }))}</h1>
            <p class="ob-lede">${escapeHtml(t(`ob.unsupported.${state.provider.reason}`))}</p>
            <div class="ob-actions">
                <button class="ob-primary" data-action="back">${escapeHtml(t("ob.back_to_address"))}</button>
            </div>`,

        done: () => `
            <h1 id="ob-title">${escapeHtml(t("ob.done.title"))}</h1>
            <p class="ob-lede">${escapeHtml(t("ob.done.body", { email: state.connectedEmail }))}</p>
            <div class="ob-actions">
                <button class="ob-primary" data-action="finish">${escapeHtml(t(cancelable ? "ob.done.button_add" : "ob.done.button"))}</button>
            </div>`,
    };

    const bindCommon = (el) => {
        el.querySelectorAll('[data-action="back"]').forEach(b => b.onclick = () => go("address"));
    };

    const BINDERS = {
        welcome: (el) => {
            el.querySelector('[data-action="start"]').onclick = () => go("address");
            el.querySelector("#ob-lang-select").onchange = (e) => {
                setLang(e.target.value);
                // Labels outside the step (stage list, window controls) too.
                root.remove();
                showOnboarding(onSuccess, cancelable);
            };
        },

        address: (el) => {
            const input = el.querySelector("#ob-email");
            el.querySelector('[data-action="google"]').onclick = () => {
                state.email = input.value.trim();
                go("google", { provider: detectProvider("x@gmail.com") });
            };
            el.querySelector("form").onsubmit = (e) => {
                e.preventDefault();
                const email = input.value.trim();
                if (!isEmailAddress(email)) {
                    state.email = email;
                    state.error = t("ob.error.invalid_email");
                    render();
                    return;
                }
                const provider = detectProvider(email);
                const next = provider.kind === "google" ? "google" : provider.kind === "unsupported" ? "unsupported" : "password";
                secretDraft = "";
                go(next, { email, provider, server: null, showServer: false });
            };
        },

        google: (el) => {
            bindCommon(el);
            el.querySelector('[data-action="google-signin"]').onclick = async () => {
                state.busy = true;
                state.error = null;
                render();
                try {
                    const account = await addGmailAccount();
                    go("done", { connectedEmail: account?.email || state.email, connectedAccount: account });
                } catch (err) {
                    console.error("Google sign-in failed", err);
                    state.busy = false;
                    state.error = t("ob.error.google");
                    render();
                }
            };
        },

        password: (el) => {
            bindCommon(el);
            const password = el.querySelector("#ob-password");
            password.value = secretDraft;
            password.oninput = () => { secretDraft = password.value; };
            const reveal = el.querySelector('[data-action="reveal"]');
            reveal.onclick = () => {
                const show = password.type === "password";
                password.type = show ? "text" : "password";
                reveal.textContent = t(show ? "ob.password.hide" : "ob.password.show");
                reveal.setAttribute("aria-pressed", String(show));
                password.focus();
            };
            const details = el.querySelector(".ob-server");
            details.ontoggle = () => { state.showServer = details.open; };
            el.querySelector("form").onsubmit = (e) => {
                e.preventDefault();
                connectWithPassword(el);
            };
        },

        unsupported: bindCommon,

        done: (el) => {
            el.querySelector('[data-action="finish"]').onclick = () => {
                root.remove();
                onSuccess(state.connectedAccount);
            };
        },
    };

    const readServerFields = (el) => ({
        imapHost: el.querySelector("#ob-imap-host").value.trim(),
        imapPort: parseInt(el.querySelector("#ob-imap-port").value, 10) || 993,
        smtpHost: el.querySelector("#ob-smtp-host").value.trim(),
        smtpPort: parseInt(el.querySelector("#ob-smtp-port").value, 10) || 587,
        username: el.querySelector("#ob-username").value.trim() || state.email,
    });

    const payloadFor = (server, secret) => ({
        email: state.email,
        displayName: state.name || null,
        password: secret,
        username: server.username,
        imapHost: server.imapHost,
        imapPort: server.imapPort,
        smtpHost: server.smtpHost,
        smtpPort: server.smtpPort,
    });

    // For unknown domains, try the usual server names before bothering the
    // person with server settings. A rejected password proves the server is
    // right, so stop there instead of trying others with the same password.
    async function findServer(secret, base) {
        let lastError = null;
        for (const candidate of state.provider.candidates) {
            const server = { ...base, ...candidate };
            try {
                await testImapCredentials(payloadFor(server, secret));
                return { server };
            } catch (err) {
                lastError = err;
                if (classifyConnectError(err) === "auth") return { server, error: err };
            }
        }
        return { error: lastError, notFound: true };
    }

    async function connectWithPassword(el) {
        const secret = secretDraft = el.querySelector("#ob-password").value;
        state.name = el.querySelector("#ob-name").value.trim();
        if (!secret) {
            state.error = t("ob.error.no_password");
            render();
            return;
        }
        const typed = readServerFields(el);
        state.server = typed;
        state.busy = true;
        state.error = null;
        render();

        let server = typed;
        try {
            if (state.provider.unknown && !state.showServer) {
                const found = await findServer(secret, typed);
                if (found.server) server = state.server = { ...typed, ...found.server };
                if (found.notFound) {
                    throw Object.assign(new Error(String(found.error)), { notFound: true });
                }
                if (found.error) throw found.error;
            }
            const account = await addImapAccount(payloadFor(server, secret));
            secretDraft = "";
            go("done", { connectedEmail: account?.email || state.email, connectedAccount: account });
        } catch (err) {
            console.error("Connecting account failed", err);
            state.busy = false;
            if (err?.notFound) {
                state.showServer = true;
                state.error = t("ob.error.not_found", { domain: domainOf(state.email) });
            } else {
                const kind = classifyConnectError(err);
                const withHint = kind === "auth" && state.provider.hint?.startsWith("enable_imap") ? "ob.error.auth_enable" :
                    kind === "auth" && state.provider.hint?.startsWith("app_password") ? "ob.error.auth_app" : `ob.error.${kind}`;
                state.error = t(withHint, { provider: providerName(), detail: String(err?.message || err) });
            }
            render();
        }
    }

    render();
}

export function hideOnboarding() {
    document.getElementById("verdant-onboarding")?.remove();
}
