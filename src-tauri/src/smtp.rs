use anyhow::{anyhow, Result};
use lettre::{
    message::{header::ContentType, Mailbox, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use serde::{Deserialize, Serialize};

use crate::credentials;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingMessage {
    pub from_name: String,
    pub from_email: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
}

pub async fn send(
    account_id: &str,
    smtp_host: &str,
    smtp_port: u16,
    use_tls: bool,
    username: &str,
    msg: &OutgoingMessage,
) -> Result<()> {
    let password = credentials::load_password(account_id)?;

    let from: Mailbox = format!("{} <{}>", msg.from_name, msg.from_email)
        .parse()
        .map_err(|e| anyhow!("invalid From address: {e}"))?;

    let mut email_builder = Message::builder().from(from);

    for to_addr in &msg.to {
        let mbox: Mailbox = to_addr.parse().map_err(|e| anyhow!("invalid To: {e}"))?;
        email_builder = email_builder.to(mbox);
    }
    for cc_addr in &msg.cc {
        let mbox: Mailbox = cc_addr.parse().map_err(|e| anyhow!("invalid Cc: {e}"))?;
        email_builder = email_builder.cc(mbox);
    }

    email_builder = email_builder.subject(&msg.subject);

    if let Some(irt) = &msg.in_reply_to {
        email_builder = email_builder.in_reply_to(irt.clone());
    }
    if let Some(refs) = &msg.references {
        email_builder = email_builder.references(refs.clone());
    }

    let email = email_builder
        .body(msg.body.clone())
        .map_err(|e| anyhow!("build email: {e}"))?;

    let creds = Credentials::new(username.to_string(), password);

    if use_tls {
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(smtp_host)
            .map_err(|e| anyhow!("smtp relay: {e}"))?
            .port(smtp_port)
            .credentials(creds)
            .build();
        mailer.send(email).await.map_err(|e| anyhow!("smtp send: {e}"))?;
    } else {
        let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(smtp_host)
            .map_err(|e| anyhow!("smtp starttls relay: {e}"))?
            .port(smtp_port)
            .credentials(creds)
            .build();
        mailer.send(email).await.map_err(|e| anyhow!("smtp send: {e}"))?;
    }

    Ok(())
}
