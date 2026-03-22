use crate::crypto::decrypt_password;
use crate::db::Account;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use lettre::{
    transport::smtp::authentication::Credentials,
    Address, Message, SmtpTransport, Transport,
};

pub struct SmtpCredentials {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub username: String,
    pub password: String,
    pub from_email: String,
}

impl SmtpCredentials {
    pub fn from_account(account: &Account) -> Result<Self, String> {
        let smtp_host = account.smtp_host.clone()
            .ok_or_else(|| "Missing SMTP host".to_string())?;
        let smtp_port = account.smtp_port
            .ok_or_else(|| "Missing SMTP port".to_string())? as u16;
        let username = account.username.clone()
            .ok_or_else(|| "Missing SMTP username".to_string())?;
        let encrypted_password = account.encrypted_password.clone()
            .ok_or_else(|| "Missing encrypted password".to_string())?;
        let password = decrypt_password(&encrypted_password)?;

        Ok(SmtpCredentials {
            smtp_host,
            smtp_port,
            username: username.clone(),
            password,
            from_email: account.email.clone(),
        })
    }
}

#[derive(serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SmtpAttachment {
    pub filename: String,
    #[serde(default)]
    pub content_type: String,
    pub data_base64: String,
}

pub fn send_imap_email(
    account: &Account,
    to: &str,
    cc: &str,
    subject: &str,
    body_plain: &str,
    body_html: Option<&str>,
    attachments: Vec<SmtpAttachment>,
    in_reply_to: Option<String>,
    references: Option<String>,
) -> Result<(), String> {
    let creds = SmtpCredentials::from_account(account)?;

    let boundary_alt = "verdant-alt-smtp-001";
    let boundary_mix = "verdant-mix-smtp-001";
    let date = chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S +0000").to_string();

    let from_display = if let Some(name) = &account.display_name {
        format!("{} <{}>", name, creds.from_email)
    } else {
        creds.from_email.clone()
    };

    let mut headers = format!(
        "From: {}\r\nTo: {}\r\nDate: {}\r\nSubject: {}\r\nMIME-Version: 1.0\r\n",
        from_display, to, date, subject
    );
    if !cc.trim().is_empty() {
        headers.push_str(&format!("Cc: {}\r\n", cc));
    }
    if let Some(irt) = &in_reply_to {
        if !irt.trim().is_empty() {
            headers.push_str(&format!("In-Reply-To: {}\r\n", irt));
        }
    }
    if let Some(refs) = &references {
        if !refs.trim().is_empty() {
            headers.push_str(&format!("References: {}\r\n", refs));
        }
    }

    let body = if attachments.is_empty() {
        if let Some(html) = body_html {
            headers.push_str(&format!(
                "Content-Type: multipart/alternative; boundary=\"{}\"\r\n\r\n",
                boundary_alt
            ));
            format!(
                "--{b}\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\n{plain}\r\n\
                 --{b}\r\nContent-Type: text/html; charset=UTF-8\r\n\r\n{html}\r\n\
                 --{b}--\r\n",
                b = boundary_alt, plain = body_plain, html = html
            )
        } else {
            headers.push_str("Content-Type: text/plain; charset=UTF-8\r\n\r\n");
            format!("{}\r\n", body_plain)
        }
    } else {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        headers.push_str(&format!(
            "Content-Type: multipart/mixed; boundary=\"{}\"\r\n\r\n",
            boundary_mix
        ));
        let mut body_part = format!(
            "--{b}\r\nContent-Type: multipart/alternative; boundary=\"{ba}\"\r\n\r\n\
             --{ba}\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\n{plain}\r\n",
            b = boundary_mix, ba = boundary_alt, plain = body_plain
        );
        if let Some(html) = body_html {
            body_part.push_str(&format!(
                "--{ba}\r\nContent-Type: text/html; charset=UTF-8\r\n\r\n{html}\r\n",
                ba = boundary_alt, html = html
            ));
        }
        body_part.push_str(&format!("--{}--\r\n", boundary_alt));

        for att in &attachments {
            let bytes = STANDARD.decode(&att.data_base64)
                .map_err(|e| format!("Attachment decode error: {}", e))?;
            let encoded = STANDARD.encode(&bytes);
            let ct = if att.content_type.trim().is_empty() {
                "application/octet-stream"
            } else {
                &att.content_type
            };
            body_part.push_str(&format!(
                "--{b}\r\nContent-Type: {ct}; name=\"{name}\"\r\n\
                 Content-Transfer-Encoding: base64\r\n\
                 Content-Disposition: attachment; filename=\"{name}\"\r\n\r\n{data}\r\n",
                b = boundary_mix, ct = ct, name = att.filename, data = encoded
            ));
        }
        body_part.push_str(&format!("--{}--\r\n", boundary_mix));
        body_part
    };

    let raw_message = format!("{}{}", headers, body);

    let from_addr: Address = creds.from_email.parse()
        .map_err(|e| format!("Invalid from address: {}", e))?;

    let mut to_addrs: Vec<Address> = Vec::new();
    for addr in to.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let clean = addr.split('<').last().unwrap_or(addr)
            .trim_matches('>').trim();
        to_addrs.push(clean.parse().map_err(|e| format!("Invalid to address '{}': {}", clean, e))?);
    }
    for addr in cc.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let clean = addr.split('<').last().unwrap_or(addr)
            .trim_matches('>').trim();
        to_addrs.push(clean.parse().map_err(|e| format!("Invalid cc address '{}': {}", clean, e))?);
    }

    if to_addrs.is_empty() {
        return Err("No valid recipients".to_string());
    }

    let smtp_creds = Credentials::new(creds.username.clone(), creds.password.clone());
    let transport = if creds.smtp_port == 465 {
        SmtpTransport::relay(&creds.smtp_host)
            .map_err(|e| e.to_string())?
            .port(creds.smtp_port)
            .credentials(smtp_creds)
            .build()
    } else {
        SmtpTransport::starttls_relay(&creds.smtp_host)
            .map_err(|e| e.to_string())?
            .port(creds.smtp_port)
            .credentials(smtp_creds)
            .build()
    };

    transport.send_raw(
        &lettre::address::Envelope::new(
            Some(from_addr),
            to_addrs,
        ).map_err(|e| e.to_string())?,
        raw_message.as_bytes(),
    ).map_err(|e| e.to_string())?;

    Ok(())
}