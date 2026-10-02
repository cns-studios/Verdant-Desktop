use super::TlsSession;

pub struct Folder {
    pub name: String,
    pub(crate) special: Option<&'static str>,
    pub(crate) selectable: bool,
}

pub fn list_folders(session: &mut TlsSession) -> Result<Vec<Folder>, String> {
    use imap::types::NameAttribute;
    let names = session
        .list(None, Some("*"))
        .map_err(|e| format!("IMAP LIST error: {}", e))?;
    Ok(names
        .iter()
        .map(|n| {
            let mut special = None;
            let mut selectable = true;
            for attr in n.attributes() {
                match attr {
                    NameAttribute::NoSelect => selectable = false,
                    NameAttribute::Custom(flag) => {
                        special = match flag.to_ascii_lowercase().as_str() {
                            "\\sent" => Some("SENT"),
                            "\\drafts" => Some("DRAFT"),
                            "\\trash" => Some("TRASH"),
                            "\\archive" => Some("ARCHIVE"),
                            "\\all" => Some("ALL"),
                            "\\junk" => Some("JUNK"),
                            "\\noselect" | "\\nonexistent" => {
                                selectable = false;
                                special
                            }
                            _ => special,
                        };
                    }
                    _ => {}
                }
            }
            Folder { name: n.name().to_string(), special, selectable }
        })
        .collect())
}

pub(crate) fn decode_imap_utf7(input: &str) -> String {
    use base64::Engine as _;
    let mut out = String::new();
    let mut rest = input;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        let Some(end) = after.find('-') else {
            out.push_str(&rest[pos..]);
            return out;
        };
        let chunk = &after[..end];
        if chunk.is_empty() {
            out.push('&');
        } else {
            let decoded = base64::engine::general_purpose::STANDARD_NO_PAD
                .decode(chunk.replace(',', "/"))
                .ok()
                .filter(|bytes| bytes.len() % 2 == 0);
            match decoded {
                Some(bytes) => {
                    let units: Vec<u16> = bytes.chunks(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                    out.push_str(&String::from_utf16_lossy(&units));
                }
                None => {
                    out.push('&');
                    out.push_str(chunk);
                    out.push('-');
                }
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

pub fn imap_folder_for_mailbox(mailbox: &str, folders: &[Folder]) -> Option<String> {
    let target = mailbox.to_uppercase();
    let selectable = || folders.iter().filter(|f| f.selectable);

    for folder in selectable() {
        if decode_imap_utf7(&folder.name).to_uppercase() == target {
            return Some(folder.name.clone());
        }
    }

    let roles: &[&str] = match target.as_str() {
        "SENT" => &["SENT"],
        "DRAFT" => &["DRAFT"],
        "TRASH" => &["TRASH"],
        "ARCHIVE" => &["ARCHIVE", "ALL"],
        _ => &[],
    };
    for role in roles {
        if let Some(folder) = selectable().find(|f| f.special == Some(*role)) {
            return Some(folder.name.clone());
        }
    }

    let candidates: &[&str] = match target.as_str() {
        "SENT" => &[
            "SENT", "SENT ITEMS", "SENT MESSAGES",
            "GESENDET", "GESENDETE ELEMENTE", "GESENDETE OBJEKTE",
            "[GMAIL]/SENT MAIL", "INBOX.SENT",
        ],
        "DRAFT" => &[
            "DRAFTS", "DRAFT", "ENTW\u{00DC}RFE",
            "[GMAIL]/DRAFTS", "INBOX.DRAFTS",
        ],
        "ARCHIVE" => &[
            "ARCHIVE", "ALL MAIL", "ARCHIV",
            "[GMAIL]/ALL MAIL", "INBOX.ARCHIVE",
        ],
        "TRASH" => &[
            "TRASH", "DELETED", "DELETED MESSAGES", "DELETED ITEMS",
            "PAPIERKORB", "GEL\u{00D6}SCHT", "GEL\u{00D6}SCHTE ELEMENTE",
            "[GMAIL]/TRASH", "INBOX.TRASH",
        ],
        _ => return None,
    };

    for candidate in candidates {
        for folder in selectable() {
            if decode_imap_utf7(&folder.name).to_uppercase() == *candidate {
                return Some(folder.name.clone());
            }
        }
    }

    for folder in selectable() {
        if decode_imap_utf7(&folder.name).to_uppercase().contains(&target) {
            return Some(folder.name.clone());
        }
    }

    None
}
