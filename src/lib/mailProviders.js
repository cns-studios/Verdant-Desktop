const GOOGLE = { kind: "google", name: "Google" };

const PRESETS = [
    { domains: ["gmail.com", "googlemail.com"], ...GOOGLE },
    {
        domains: ["outlook.com", "outlook.de", "hotmail.com", "hotmail.de", "live.com", "live.de", "msn.com"],
        kind: "unsupported", name: "Outlook", reason: "microsoft",
    },
    { domains: ["proton.me", "protonmail.com", "protonmail.ch", "pm.me"], kind: "unsupported", name: "Proton Mail", reason: "proton" },
    {
        domains: ["tuta.com", "tuta.io", "tutanota.com", "tutanota.de", "tutamail.com", "keemail.me"],
        kind: "unsupported", name: "Tuta", reason: "tuta",
    },
    {
        domains: ["gmx.de", "gmx.net", "gmx.at", "gmx.ch"],
        kind: "password", name: "GMX", hint: "enable_imap_gmx",
        imapHost: "imap.gmx.net", smtpHost: "mail.gmx.net", smtpPort: 587,
    },
    {
        domains: ["gmx.com", "gmx.us", "gmx.co.uk", "gmx.fr", "gmx.es"],
        kind: "password", name: "GMX", hint: "enable_imap_gmx",
        imapHost: "imap.gmx.com", smtpHost: "mail.gmx.com", smtpPort: 587,
    },
    {
        domains: ["web.de"],
        kind: "password", name: "WEB.DE", hint: "enable_imap_webde",
        imapHost: "imap.web.de", smtpHost: "smtp.web.de", smtpPort: 587,
    },
    {
        domains: ["t-online.de", "magenta.de"],
        kind: "password", name: "Telekom", hint: "telekom",
        imapHost: "secureimap.t-online.de", smtpHost: "securesmtp.t-online.de", smtpPort: 465,
    },
    {
        domains: ["freenet.de"],
        kind: "password", name: "freenet", hint: "enable_imap_generic",
        imapHost: "mx.freenet.de", smtpHost: "mx.freenet.de", smtpPort: 587,
    },
    {
        domains: ["yahoo.com", "yahoo.de", "yahoo.co.uk", "yahoo.fr", "ymail.com", "rocketmail.com"],
        kind: "password", name: "Yahoo", hint: "app_password_yahoo",
        imapHost: "imap.mail.yahoo.com", smtpHost: "smtp.mail.yahoo.com", smtpPort: 465,
    },
    {
        domains: ["aol.com", "aol.de"],
        kind: "password", name: "AOL", hint: "app_password_aol",
        imapHost: "imap.aol.com", smtpHost: "smtp.aol.com", smtpPort: 465,
    },
    {
        domains: ["icloud.com", "me.com", "mac.com"],
        kind: "password", name: "iCloud", hint: "app_password_icloud", usernameIsLocalPart: true,
        imapHost: "imap.mail.me.com", smtpHost: "smtp.mail.me.com", smtpPort: 587,
    },
    {
        domains: ["fastmail.com", "fastmail.fm"],
        kind: "password", name: "Fastmail", hint: "app_password_fastmail",
        imapHost: "imap.fastmail.com", smtpHost: "smtp.fastmail.com", smtpPort: 465,
    },
    { domains: ["posteo.de", "posteo.net"], kind: "password", name: "Posteo", imapHost: "posteo.de", smtpHost: "posteo.de", smtpPort: 587 },
    { domains: ["mailbox.org"], kind: "password", name: "mailbox.org", imapHost: "imap.mailbox.org", smtpHost: "smtp.mailbox.org", smtpPort: 587 },
    { domains: ["mail.de"], kind: "password", name: "mail.de", imapHost: "imap.mail.de", smtpHost: "smtp.mail.de", smtpPort: 465 },
    { domains: ["zoho.com", "zohomail.com"], kind: "password", name: "Zoho Mail", imapHost: "imap.zoho.com", smtpHost: "smtp.zoho.com", smtpPort: 465 },
];

const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/;

export function isEmailAddress(value) {
    return EMAIL_PATTERN.test(String(value || "").trim());
}

export function domainOf(email) {
    return String(email || "").trim().toLowerCase().split("@")[1] || "";
}

export function detectProvider(email) {
    const domain = domainOf(email);
    const preset = PRESETS.find(p => p.domains.includes(domain));
    if (preset) {
        const { domains, ...rest } = preset;
        return { imapPort: 993, ...rest };
    }
    return {
        kind: "password",
        unknown: true,
        name: domain,
        imapPort: 993,
        smtpPort: 587,
        candidates: [
            { imapHost: `imap.${domain}`, smtpHost: `smtp.${domain}` },
            { imapHost: `mail.${domain}`, smtpHost: `mail.${domain}` },
            { imapHost: domain, smtpHost: domain },
        ],
    };
}

export function usernameFor(provider, email) {
    const clean = String(email || "").trim();
    return provider?.usernameIsLocalPart ? clean.split("@")[0] : clean;
}

export function classifyConnectError(error) {
    const text = String(error || "");
    if (/UNIQUE constraint failed: accounts\.email/i.test(text)) return "duplicate";
    if (/login error|authenticat|invalid credentials|AUTHENTICATIONFAILED/i.test(text)) return "auth";
    if (/TLS/i.test(text)) return "security";
    if (/DNS resolution|connect error|connect failed|timed out|No route|refused|unreachable/i.test(text)) return "network";
    return "unknown";
}
