pub(crate) const PERSONAL_PROVIDERS: &[&str] = &[
    "gmail.com", "googlemail.com", "outlook.com", "hotmail.com", "live.com",
    "msn.com", "yahoo.com", "yahoo.co.uk", "icloud.com", "me.com", "mac.com",
    "aol.com", "protonmail.com", "proton.me", "gmx.com", "gmx.net", "web.de",
    "zoho.com", "fastmail.com", "mail.com",
];

pub(crate) fn sender_address(sender: &str) -> String {
    if let (Some(start), Some(end)) = (sender.find('<'), sender.rfind('>')) {
        if end > start {
            return sender[start + 1..end].trim().to_lowercase();
        }
    }
    sender.trim().to_lowercase()
}

pub(crate) fn sender_domain(sender: &str) -> Option<String> {
    let address = sender_address(sender);
    address
        .rsplit_once('@')
        .map(|(_, d)| d.trim().trim_end_matches('.').to_string())
        .filter(|d| !d.is_empty() && d.contains('.'))
        .filter(|d| d.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.'))
}

pub(crate) fn domain_brand(domain: &str) -> String {
    const KNOWN_SUFFIXES: &[&str] = &[
        "co.uk", "co.jp", "com.au", "com.br", "co.in", "com.cn", "co.nz",
    ];
    let mut host = domain.to_string();
    for suffix in KNOWN_SUFFIXES {
        if let Some(stripped) = host.strip_suffix(&format!(".{suffix}")) {
            host = stripped.to_string();
            break;
        }
    }
    let mut parts: Vec<&str> = host.split('.').collect();
    if parts.len() > 1 {
        parts.pop();
    }
    parts.pop().unwrap_or(domain).to_string()
}

pub(crate) fn slugify(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

pub(crate) enum SenderClass {
    Personal,
    Domain { domain: String, bulk: bool },
    Unknown,
}

pub(crate) fn classify_sender(sender: &str, list_unsubscribe: &str) -> SenderClass {
    match sender_domain(sender) {
        Some(domain) if PERSONAL_PROVIDERS.contains(&domain.as_str()) => SenderClass::Personal,
        Some(domain) => SenderClass::Domain { domain, bulk: !list_unsubscribe.trim().is_empty() },
        None => SenderClass::Unknown,
    }
}
