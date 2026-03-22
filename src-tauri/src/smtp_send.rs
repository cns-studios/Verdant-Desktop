use crate::crypto::decrypt_password;
use crate::db::Account;
use lettre::{
    message::{header::ContentType, Attachment, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    Message, SmtpTransport, Transport,
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
) -> Result<(), String> {
    let creds = SmtpCredentials::from_account(account)?;

    let from_mailbox = if let Some(name) = &account.display_name {
        format!("{} <{}>", name, creds.from_email)
    } else {
        creds.from_email.clone()
    }
    .parse::<lettre::message::Mailbox>()
    .map_err(|e| format!("Invalid from address '{}': {}", creds.from_email, e))?;

    let mut builder = Message::builder()
        .from(from_mailbox)
        .subject(subject);

    
    for addr in to.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let mailbox = addr.parse::<lettre::message::Mailbox>()
            .map_err(|e| format!("Invalid recipient '{}': {}", addr, e))?;
        builder = builder.to(mailbox);
    }

    
    for addr in cc.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let mailbox = addr.parse::<lettre::message::Mailbox>()
            .map_err(|e| format!("Invalid cc '{}': {}", addr, e))?;
        builder = builder.cc(mailbox);
    }

    
    let message = if attachments.is_empty() {
        if let Some(html) = body_html {
            let multipart = MultiPart::alternative()
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_PLAIN)
                        .body(body_plain.to_string()),
                )
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_HTML)
                        .body(html.to_string()),
                );
            builder.multipart(multipart).map_err(|e| e.to_string())?
        } else {
            builder
                .header(ContentType::TEXT_PLAIN)
                .body(body_plain.to_string())
                .map_err(|e| e.to_string())?
        }
    } else {
        use base64::{engine::general_purpose::STANDARD, Engine as _};

        let body_part = if let Some(html) = body_html {
            MultiPart::alternative()
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_PLAIN)
                        .body(body_plain.to_string()),
                )
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_HTML)
                        .body(html.to_string()),
                )
        } else {
            MultiPart::alternative()
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_PLAIN)
                        .body(body_plain.to_string()),
                )
        };

        let mut mixed = MultiPart::mixed().multipart(body_part);

        for att in attachments {
            let bytes = STANDARD.decode(&att.data_base64)
                .map_err(|e| format!("Attachment decode error: {}", e))?;
            let ct = att.content_type.parse::<ContentType>()
                .unwrap_or(ContentType::TEXT_PLAIN);
            let attachment_part = Attachment::new(att.filename)
                .body(bytes, ct);
            mixed = mixed.singlepart(attachment_part);
        }

        builder.multipart(mixed).map_err(|e| e.to_string())?
    };

    

    let smtp_creds = Credentials::new(creds.username, creds.password);
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

    transport.send(&message).map_err(|e| e.to_string())?;
    Ok(())
}
