const { invoke } = window.__TAURI__.core;

let currentEmails = [];

function renderOnboarding(message, showConnect = true) {
        const list = document.querySelector(".email-list");
        if (!list) return;

        list.innerHTML = `
            <div class="email-item active" style="cursor: default;">
                <div class="email-item-inner" style="padding: 16px;">
                    <div class="email-top">
                        <span class="email-sender">Verdant Setup</span>
                        <span class="email-time">Now</span>
                    </div>
                    <div class="email-subject">Connect Gmail To Get Started</div>
                    <div class="email-preview">${escapeHtml(message)}</div>
                    ${showConnect ? '<button id="connect-gmail-btn" class="send-btn" style="margin-top: 12px; width: auto;">Connect Gmail</button>' : ''}
                </div>
            </div>
        `;

        if (showConnect) {
                const button = document.getElementById("connect-gmail-btn");
                if (button) {
                        button.addEventListener("click", async () => {
                                button.disabled = true;
                                button.textContent = "Connecting...";
                                try {
                                        await invoke("sync_emails");
                                        await refreshInbox();
                                } catch (error) {
                                        renderOnboarding(String(error), true);
                                }
                        });
                }
        }
}

function escapeHtml(input) {
    if (!input) return "";
    return input
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/\"/g, "&quot;")
        .replace(/'/g, "&#039;");
}

function shortDate(raw) {
    const d = new Date(raw);
    if (Number.isNaN(d.getTime())) return raw || "";
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function renderReadingPane(email) {
    const subject = document.querySelector(".reading-subject");
    const from = document.querySelector(".meta-from");
    const date = document.querySelector(".meta-date");
    const body = document.querySelector(".email-body-text");

    if (subject) subject.textContent = email.subject || "(No Subject)";
    if (from) from.textContent = email.sender || "Unknown Sender";
    if (date) date.textContent = email.date || "";
    if (body) body.innerHTML = email.body_html || `<p>${escapeHtml(email.snippet || "")}</p>`;
}

function renderEmailList(emails) {
    const list = document.querySelector(".email-list");
    if (!list) return;
    list.innerHTML = "";

    emails.forEach((email, index) => {
        const item = document.createElement("div");
        item.className = `email-item ${email.is_read ? "" : "unread"} ${index === 0 ? "active" : ""}`.trim();

        item.innerHTML = `
            ${email.is_read ? "" : '<div class="unread-dot"></div>'}
            <div class="email-item-inner">
                <div class="email-top">
                    <span class="email-sender">${escapeHtml(email.sender || "Unknown Sender")}</span>
                    <span class="email-time">${escapeHtml(shortDate(email.date))}</span>
                </div>
                <div class="email-subject">${escapeHtml(email.subject || "(No Subject)")}</div>
                <div class="email-preview">${escapeHtml(email.snippet || "")}</div>
            </div>
        `;

        item.addEventListener("click", () => {
            document.querySelectorAll(".email-item").forEach((el) => el.classList.remove("active"));
            item.classList.add("active");
            item.classList.remove("unread");
            const dot = item.querySelector(".unread-dot");
            if (dot) dot.remove();
            renderReadingPane(email);
        });

        list.appendChild(item);
    });

    if (emails.length > 0) {
        renderReadingPane(emails[0]);
    }
}

async function refreshInbox() {
    await invoke("sync_emails");
    currentEmails = await invoke("get_emails");
    if (!currentEmails || currentEmails.length === 0) {
        renderOnboarding("Connected successfully, but your inbox returned no messages yet.", false);
        return;
    }
    renderEmailList(currentEmails);
}

async function loadCachedInbox() {
    currentEmails = await invoke("get_emails");
    if (currentEmails && currentEmails.length > 0) {
        renderEmailList(currentEmails);
        return true;
    }
    return false;
}

function bindSendButton() {
    const sendBtn = document.querySelector(".send-btn");
    if (!sendBtn) return;

    sendBtn.addEventListener("click", async () => {
        const fields = document.querySelectorAll(".modal-field input");
        const to = fields[0]?.value?.trim() || "";
        const subject = fields[2]?.value?.trim() || "";
        const body = document.querySelector(".modal-body textarea")?.value || "";

        if (!to) {
            alert("Recipient is required");
            return;
        }

        try {
            await invoke("send_email", { to, subject, body });
            if (typeof window.closeCompose === "function") {
                window.closeCompose();
            }
            fields.forEach((input) => {
                input.value = "";
            });
            const textarea = document.querySelector(".modal-body textarea");
            if (textarea) textarea.value = "";
            await refreshInbox();
        } catch (error) {
            alert(`Failed to send email: ${error}`);
        }
    });
}

document.addEventListener("DOMContentLoaded", async () => {
    bindSendButton();

    // Always clear hardcoded mock items immediately so failures never show fake mail.
    const list = document.querySelector(".email-list");
    if (list) list.innerHTML = "";

    try {
        await loadCachedInbox();
        await refreshInbox();
    } catch (error) {
        console.error("Failed to load emails:", error);
        try {
            const status = await invoke("auth_status");
            if (!status.has_client_id) {
                renderOnboarding("Missing GOOGLE_CLIENT_ID in .env. Add credentials, restart app, then connect.", false);
                return;
            }

            if (!status.connected) {
                renderOnboarding("Sign in with Gmail to sync your inbox.", true);
                return;
            }
        } catch (statusError) {
            console.error("Failed to read auth status:", statusError);
        }

        if (!currentEmails || currentEmails.length === 0) {
            renderOnboarding(String(error), true);
        }
    }
});
