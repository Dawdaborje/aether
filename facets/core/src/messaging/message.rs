//! What a plugin may ask to be sent, and the checks on it.
//!
//! One command, `communication::send`, takes the *type* of message and its content:
//!
//! ```json
//! { "type": "email", "to": ["ann@example.com"], "subject": "Hello", "text": "Hi Ann" }
//! { "type": "sms",   "to": "+2348012345678", "text": "Your code is 1234" }
//! ```
//!
//! The kernel checks it, splits a message to several recipients into one job per recipient (so a
//! retry never sends twice to someone it already reached, and recipients never see each other),
//! and the scheduler delivers each through the bridge configured for that type.

use aether_communication::{Email, Sms};
use serde_json::{Map, Value};

/// Most recipients of one `communication::send`.
pub const MAX_RECIPIENTS: usize = 50;
const MAX_SUBJECT_CHARS: usize = 300;
const MAX_BODY_BYTES: usize = 512 * 1024;
const MAX_SMS_CHARS: usize = 1600;

/// The kinds of message the kernel can route. Each needs a capability of the same name
/// (`email::send`, `sms::send`) besides `communication::send`.
pub const TYPES: &[&str] = &["email", "sms"];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct MessageError(pub String);

fn bad(message: impl Into<String>) -> MessageError {
    MessageError(message.into())
}

/// A message that passed the checks, for one recipient.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Email { to: String, reply_to: Option<String>, subject: String, text: Option<String>, html: Option<String> },
    Sms { to: String, text: String },
}

impl Message {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Email { .. } => "email",
            Self::Sms { .. } => "sms",
        }
    }

    /// The queue that delivers this kind of message.
    pub fn queue(&self) -> &'static str {
        self.kind()
    }

    /// The JSON a job carries (one recipient).
    pub fn to_payload(&self) -> Value {
        match self {
            Self::Email { to, reply_to, subject, text, html } => serde_json::json!({
                "type": "email", "to": to, "reply_to": reply_to, "subject": subject, "text": text, "html": html,
            }),
            Self::Sms { to, text } => serde_json::json!({ "type": "sms", "to": to, "text": text }),
        }
    }

    /// Read a job's payload (the output of [`to_payload`](Self::to_payload)).
    pub fn from_job(payload: &Value) -> Result<Self, MessageError> {
        let recipients = recipients(payload)?;
        let [only] = recipients.as_slice() else { return Err(bad("a queued message has exactly one recipient")) };
        Self::build(payload, only.clone())
    }

    fn build(request: &Value, to: String) -> Result<Self, MessageError> {
        let text_field = |name: &str| request.get(name).and_then(Value::as_str).filter(|t| !t.is_empty()).map(str::to_string);
        match request.get("type").and_then(Value::as_str) {
            Some("email") => {
                let (text, html) = (text_field("text"), text_field("html"));
                if text.is_none() && html.is_none() {
                    return Err(bad("an email needs `text` or `html`"));
                }
                if text.as_deref().map_or(0, str::len) + html.as_deref().map_or(0, str::len) > MAX_BODY_BYTES {
                    return Err(bad(format!("an email body is at most {MAX_BODY_BYTES} bytes")));
                }
                let subject = text_field("subject").ok_or_else(|| bad("an email needs a `subject`"))?;
                if subject.chars().count() > MAX_SUBJECT_CHARS || subject.chars().any(char::is_control) {
                    return Err(bad(format!("a subject is one line of at most {MAX_SUBJECT_CHARS} characters")));
                }
                let reply_to = match text_field("reply_to") {
                    Some(address) => Some(check_email(&address)?),
                    None => None,
                };
                Ok(Self::Email { to: check_email(&to)?, reply_to, subject, text, html })
            }
            Some("sms") => {
                let text = text_field("text").ok_or_else(|| bad("an SMS needs `text`"))?;
                if text.chars().count() > MAX_SMS_CHARS {
                    return Err(bad(format!("an SMS is at most {MAX_SMS_CHARS} characters")));
                }
                Ok(Self::Sms { to: check_phone(&to)?, text })
            }
            Some(other) => Err(bad(format!("`{other}` is not a message type the kernel can send; use one of {TYPES:?}"))),
            None => Err(bad(format!("name the message `type`: one of {TYPES:?}"))),
        }
    }

    /// Check a plugin's request and make one message per recipient.
    pub fn from_request(request: &Value) -> Result<Vec<Self>, MessageError> {
        if !request.is_object() {
            return Err(bad("a message is a JSON object"));
        }
        recipients(request)?.into_iter().map(|to| Self::build(request, to)).collect()
    }
}

/// `to` as a list: a text or a list of texts, one to [`MAX_RECIPIENTS`] of them, duplicates removed.
fn recipients(request: &Value) -> Result<Vec<String>, MessageError> {
    let mut list: Vec<String> = match request.get("to") {
        Some(Value::String(one)) => vec![one.trim().to_string()],
        Some(Value::Array(many)) => many
            .iter()
            .map(|item| item.as_str().map(|text| text.trim().to_string()).ok_or_else(|| bad("`to` holds only text")))
            .collect::<Result<_, _>>()?,
        _ => return Err(bad("`to` is required: a recipient or a list of them")),
    };
    let mut seen = std::collections::HashSet::new();
    list.retain(|item| seen.insert(item.to_ascii_lowercase()));
    if list.is_empty() || list.len() > MAX_RECIPIENTS {
        return Err(bad(format!("a message goes to 1 to {MAX_RECIPIENTS} recipients, not {}", list.len())));
    }
    Ok(list)
}

/// A plain address like `ann@example.com`: no display names, no lists, no control characters.
pub fn check_email(address: &str) -> Result<String, MessageError> {
    let ok = address.len() <= 254
        && !address.chars().any(|c| c.is_control() || c.is_whitespace() || matches!(c, '<' | '>' | ',' | ';' | '"'))
        && address.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && local.len() <= 64
                && domain.contains('.')
                && !domain.starts_with(['.', '-'])
                && !domain.ends_with(['.', '-'])
                && !domain.contains("..")
                && !domain.contains('@')
        });
    if ok { Ok(address.to_string()) } else { Err(bad(format!("`{address}` is not an email address"))) }
}

/// International form: `+` and 8 to 15 digits (spaces, dashes and brackets are dropped).
pub fn check_phone(number: &str) -> Result<String, MessageError> {
    let cleaned: String = number.chars().filter(|c| !matches!(c, ' ' | '-' | '(' | ')' | '.')).collect();
    let digits = cleaned.strip_prefix('+').unwrap_or("");
    if (8..=15).contains(&digits.len()) && digits.chars().all(|c| c.is_ascii_digit()) && !digits.starts_with('0') {
        Ok(cleaned)
    } else {
        Err(bad(format!("`{number}` is not a phone number in international form such as +2348012345678")))
    }
}

/// The bridge-facing form of an email, given who it is from.
pub fn email_for_bridge(message: &Message, from_address: String, from_name: Option<String>) -> Option<Email> {
    match message {
        Message::Email { to, reply_to, subject, text, html } => Some(Email {
            from_address,
            from_name,
            to: to.clone(),
            reply_to: reply_to.clone(),
            subject: subject.clone(),
            text: text.clone(),
            html: html.clone(),
        }),
        Message::Sms { .. } => None,
    }
}

/// The bridge-facing form of an SMS.
pub fn sms_for_bridge(message: &Message, sender: Option<String>) -> Option<Sms> {
    match message {
        Message::Sms { to, text } => Some(Sms { to: to.clone(), text: text.clone(), sender }),
        Message::Email { .. } => None,
    }
}

/// Fields the request may carry, for the error that lists them.
pub fn allowed_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "email" => &["type", "to", "subject", "text", "html", "reply_to"],
        _ => &["type", "to", "text"],
    }
}

/// Refuse fields the kernel does not know, so a plugin that tries to choose the sender (`from`)
/// finds out instead of being silently ignored. Senders are the administrator's to set.
pub fn reject_unknown_fields(request: &Map<String, Value>) -> Result<(), MessageError> {
    let kind = request.get("type").and_then(Value::as_str).unwrap_or("");
    if let Some(field) = request.keys().find(|key| !allowed_fields(kind).contains(&key.as_str())) {
        return Err(bad(format!(
            "`{field}` is not a field of a {kind} message (the sender is set in the settings, not by plugins); fields: {:?}",
            allowed_fields(kind)
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_email_to_several_people_becomes_one_message_each() {
        let messages = Message::from_request(&json!({
            "type": "email", "to": ["a@x.com", "b@x.com", "A@X.com"], "subject": "Hi", "text": "Body"
        }))
        .unwrap();
        assert_eq!(messages.len(), 2, "duplicates (ignoring case) are dropped");
        assert!(messages.iter().all(|m| m.kind() == "email" && m.queue() == "email"));
        // A job's payload reads back as the same message.
        for message in messages {
            assert_eq!(Message::from_job(&message.to_payload()).unwrap(), message);
        }
    }

    #[test]
    fn sms_numbers_are_cleaned_and_checked() {
        let sms = |to: &str| Message::from_request(&json!({ "type": "sms", "to": to, "text": "Hi" }));
        assert_eq!(sms("+234 801-234-5678").unwrap(), vec![Message::Sms { to: "+2348012345678".into(), text: "Hi".into() }]);
        for bad in ["08012345678", "+0801234567", "+12", "+1234567890123456", "+23480abc5678", ""] {
            assert!(sms(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn emails_are_checked() {
        for good in ["ann@example.com", "a.b+c@sub.example.co"] {
            assert!(check_email(good).is_ok(), "{good}");
        }
        for bad in ["", "ann", "ann@", "@x.com", "ann@x", "ann@x..com", "Ann <ann@x.com>", "a b@x.com", "a@x.com,b@x.com", "a@x.com\r\nBcc: x@y.com"] {
            assert!(check_email(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn content_is_required_and_bounded() {
        let email = |extra: Value| {
            let mut base = json!({ "type": "email", "to": "a@x.com", "subject": "S", "text": "T" });
            for (key, value) in extra.as_object().unwrap() {
                base[key] = value.clone();
            }
            Message::from_request(&base)
        };
        assert!(email(json!({})).is_ok());
        assert!(email(json!({ "text": null, "html": null })).is_err(), "no body");
        assert!(email(json!({ "subject": "" })).is_err());
        assert!(email(json!({ "subject": "two\nlines" })).is_err(), "a subject cannot carry a header injection");
        assert!(email(json!({ "reply_to": "nope" })).is_err());
        assert!(email(json!({ "text": "x".repeat(MAX_BODY_BYTES + 1) })).is_err());
        assert!(Message::from_request(&json!({ "type": "sms", "to": "+2348012345678", "text": "x".repeat(MAX_SMS_CHARS + 1) })).is_err());
    }

    #[test]
    fn the_recipient_list_and_type_are_checked() {
        assert!(Message::from_request(&json!({ "type": "fax", "to": "x", "text": "t" })).is_err());
        assert!(Message::from_request(&json!({ "to": "a@x.com", "subject": "S", "text": "T" })).is_err(), "no type");
        assert!(Message::from_request(&json!({ "type": "email", "subject": "S", "text": "T" })).is_err(), "no to");
        let many: Vec<String> = (0..=MAX_RECIPIENTS).map(|n| format!("u{n}@x.com")).collect();
        assert!(Message::from_request(&json!({ "type": "email", "to": many, "subject": "S", "text": "T" })).is_err());
        assert!(Message::from_request(&json!({ "type": "email", "to": [1], "subject": "S", "text": "T" })).is_err());
        assert!(Message::from_request(&json!("text")).is_err());
    }

    #[test]
    fn a_plugin_cannot_choose_the_sender() {
        let request = json!({ "type": "email", "to": "a@x.com", "subject": "S", "text": "T", "from": "ceo@bank.com" });
        let error = reject_unknown_fields(request.as_object().unwrap()).unwrap_err();
        assert!(error.0.contains("`from`") && error.0.contains("settings"), "{error}");
        assert!(reject_unknown_fields(json!({ "type": "sms", "to": "+2348012345678", "text": "t", "sender": "X" }).as_object().unwrap()).is_err());
        assert!(reject_unknown_fields(json!({ "type": "sms", "to": "+2348012345678", "text": "t" }).as_object().unwrap()).is_ok());
    }
}
