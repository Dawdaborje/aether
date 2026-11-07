//! Email through any SMTP server (your own, or a provider's SMTP relay).

use std::time::Duration;

use aether_communication::{ConfigError, Email, EmailBridge, Field, REQUEST_TIMEOUT_SECS, SendError, Spec, Values};
use async_trait::async_trait;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, MultiPart, SinglePart, header::ContentType},
    transport::smtp::authentication::Credentials,
};

pub const SPEC: Spec = Spec {
    key: "smtp",
    label: "SMTP",
    account: &[
        Field { name: "host", label: "Server", secret: false, required: true, default: None },
        Field { name: "username", label: "Username", secret: false, required: false, default: None },
        Field { name: "password", label: "Password", secret: true, required: false, default: None },
    ],
    options: &[
        Field { name: "security", label: "Security (starttls, tls or none)", secret: false, required: false, default: Some("starttls") },
        Field { name: "port", label: "Port (587 for starttls, 465 for tls, 25 for none)", secret: false, required: false, default: None },
    ],
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Security {
    StartTls,
    Tls,
    None,
}

pub struct Smtp {
    host: String,
    port: u16,
    security: Security,
    credentials: Option<Credentials>,
}

impl Smtp {
    pub fn new(values: &Values) -> Result<Self, ConfigError> {
        let host = aether_communication::required(values, "smtp", "host")?.to_string();
        let security = match values.get("security").map(String::as_str).filter(|s| !s.is_empty()).unwrap_or("starttls") {
            "starttls" => Security::StartTls,
            "tls" => Security::Tls,
            "none" => Security::None,
            other => return Err(ConfigError(format!("smtp: security `{other}` is not starttls, tls or none"))),
        };
        let port = match values.get("port").filter(|p| !p.is_empty()) {
            Some(port) => port.parse().map_err(|_| ConfigError(format!("smtp: `{port}` is not a port number")))?,
            None => match security {
                Security::StartTls => 587,
                Security::Tls => 465,
                Security::None => 25,
            },
        };
        let credentials = match (values.get("username").filter(|u| !u.is_empty()), values.get("password")) {
            (Some(user), Some(password)) => Some(Credentials::new(user.clone(), password.clone())),
            (Some(_), None) => return Err(ConfigError("smtp: a username needs a password".into())),
            (None, _) => None,
        };
        Ok(Self { host, port, security, credentials })
    }
}

fn mailbox(address: &str, name: Option<&str>) -> Result<Mailbox, SendError> {
    let address = address.parse().map_err(|_| SendError::permanent(format!("`{address}` is not an email address")))?;
    Ok(Mailbox::new(name.map(str::to_string), address))
}

/// The message as it goes on the wire.
pub fn build_message(email: &Email) -> Result<Message, SendError> {
    let builder = Message::builder()
        .from(mailbox(&email.from_address, email.from_name.as_deref())?)
        .to(mailbox(&email.to, None)?)
        .subject(email.subject.clone());
    let builder = match &email.reply_to {
        Some(reply_to) => builder.reply_to(mailbox(reply_to, None)?),
        None => builder,
    };
    let built = match (&email.text, &email.html) {
        (Some(text), Some(html)) => builder.multipart(MultiPart::alternative_plain_html(text.clone(), html.clone())),
        (Some(text), None) => builder.header(ContentType::TEXT_PLAIN).body(text.clone()),
        (None, Some(html)) => builder.singlepart(SinglePart::html(html.clone())),
        (None, None) => return Err(SendError::permanent("an email needs a text or an HTML body")),
    };
    built.map_err(|e| SendError::permanent(format!("the email could not be built: {e}")))
}

#[async_trait]
impl EmailBridge for Smtp {
    async fn send(&self, email: &Email) -> Result<(), SendError> {
        let message = build_message(email)?;
        let builder = match self.security {
            Security::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.host),
            Security::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&self.host),
            Security::None => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.host)),
        }
        .map_err(|e| SendError::permanent(format!("the SMTP server `{}` cannot be used: {e}", self.host)))?;
        let mut builder = builder.port(self.port).timeout(Some(Duration::from_secs(REQUEST_TIMEOUT_SECS)));
        if let Some(credentials) = &self.credentials {
            builder = builder.credentials(credentials.clone());
        }
        match builder.build().send(message).await {
            Ok(_) => Ok(()),
            Err(error) if error.is_permanent() => Err(SendError::permanent(format!("the SMTP server refused it: {error}"))),
            Err(error) => Err(SendError::transient(format!("the SMTP server could not take it: {error}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn email() -> Email {
        Email {
            from_address: "billing@acme.test".into(),
            from_name: Some("Acme Billing".into()),
            to: "ann@example.com".into(),
            reply_to: Some("help@acme.test".into()),
            subject: "Your invoice".into(),
            text: Some("Total: 500".into()),
            html: Some("<b>Total: 500</b>".into()),
        }
    }

    fn values(pairs: &[(&str, &str)]) -> Values {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn the_message_has_the_right_headers_and_both_bodies() {
        let raw = String::from_utf8(build_message(&email()).unwrap().formatted()).unwrap();
        assert!(raw.contains("From: \"Acme Billing\" <billing@acme.test>"), "{raw}");
        assert!(raw.contains("To: ann@example.com"), "{raw}");
        assert!(raw.contains("Reply-To: help@acme.test"), "{raw}");
        assert!(raw.contains("Subject: Your invoice"), "{raw}");
        assert!(raw.contains("multipart/alternative") && raw.contains("Total: 500") && raw.contains("<b>Total: 500</b>"), "{raw}");
    }

    #[test]
    fn a_bad_address_or_no_body_is_permanent() {
        let mut bad = email();
        bad.to = "not an address".into();
        assert!(build_message(&bad).unwrap_err().permanent);
        let mut empty = email();
        empty.text = None;
        empty.html = None;
        assert!(build_message(&empty).unwrap_err().permanent);
    }

    #[test]
    fn settings_are_checked_and_ports_default_by_security() {
        assert_eq!(Smtp::new(&values(&[("host", "mail.test")])).unwrap().port, 587);
        assert_eq!(Smtp::new(&values(&[("host", "mail.test"), ("security", "tls")])).unwrap().port, 465);
        assert_eq!(Smtp::new(&values(&[("host", "mail.test"), ("security", "none")])).unwrap().port, 25);
        assert_eq!(Smtp::new(&values(&[("host", "mail.test"), ("port", "2525")])).unwrap().port, 2525);
        assert!(Smtp::new(&values(&[])).is_err());
        assert!(Smtp::new(&values(&[("host", "m"), ("security", "ssl3")])).is_err());
        assert!(Smtp::new(&values(&[("host", "m"), ("port", "x")])).is_err());
        assert!(Smtp::new(&values(&[("host", "m"), ("username", "u")])).is_err(), "a username needs a password");
    }
}
