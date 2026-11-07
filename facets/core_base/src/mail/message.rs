//! A received email as the kernel hands it on: clean, small, and the same whichever provider sent it.
//!
//! Providers post their own shapes; [`parse`] turns each into an [`Inbound`]. Nothing here touches the database.
//! The HTML is cleaned once, here, before any plugin sees it (`ammonia`: no scripts, no forms, no styles, links
//! open safely). Attachments stay as raw bytes until the service has checked and stored them.

use base64::Engine as _;
use serde::Serialize;
use serde_json::{Map, Value, json};

/// Most attachments kept per message; the rest are named in `dropped_attachments`.
pub const MAX_ATTACHMENTS: usize = 20;
/// Most recipients kept.
const MAX_RECIPIENTS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Person {
    pub name: String,
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawAttachment {
    pub name: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// What the sender's mail provider says about the sender's domain: `pass`, `fail`, `none`, ...
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Auth {
    pub spf: String,
    pub dkim: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inbound {
    pub from: Person,
    pub to: Vec<Person>,
    pub cc: Vec<Person>,
    pub subject: String,
    pub text: String,
    pub html: String,
    pub message_id: String,
    pub in_reply_to: String,
    pub references: Vec<String>,
    pub received_at: String,
    pub auth: Auth,
    /// Mail that no person wrote: auto-replies, bounces, bulk and list mail.
    pub automatic: Option<String>,
    pub attachments: Vec<RawAttachment>,
    pub dropped_attachments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ParseError(pub String);

fn bad(text: &str) -> ParseError {
    ParseError(text.to_string())
}

/// The providers the intake knows how to read.
pub const PROVIDERS: [&str; 2] = ["generic", "postmark"];

pub fn parse(provider: &str, body: &Value) -> Result<Inbound, ParseError> {
    let mut message = match provider {
        "postmark" => postmark(body)?,
        "generic" => generic(body)?,
        _ => return Err(bad("unknown provider")),
    };
    message.html = clean_html(&message.html);
    if message.attachments.len() > MAX_ATTACHMENTS {
        for extra in message.attachments.split_off(MAX_ATTACHMENTS) {
            message.dropped_attachments.push(extra.name);
        }
    }
    message.to.truncate(MAX_RECIPIENTS);
    message.cc.truncate(MAX_RECIPIENTS);
    if message.from.address.is_empty() || !message.from.address.contains('@') {
        return Err(bad("the message has no sender address"));
    }
    if message.message_id.is_empty() {
        return Err(bad("the message has no Message-ID"));
    }
    Ok(message)
}

/// HTML a person can read and nothing a browser would run.
pub fn clean_html(html: &str) -> String {
    if html.trim().is_empty() {
        return String::new();
    }
    let mut builder = ammonia::Builder::default();
    builder.link_rel(Some("noopener noreferrer nofollow"));
    builder.url_schemes(["http", "https", "mailto", "cid"].into_iter().collect());
    builder.clean(html).to_string()
}

fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().trim().to_string()
}

/// `<id@host>` with the brackets and spaces gone, lower-cased: the form Message-IDs are compared in.
pub fn bare_id(id: &str) -> String {
    id.trim().trim_start_matches('<').trim_end_matches('>').trim().to_ascii_lowercase()
}

fn ids(text: &str) -> Vec<String> {
    text.split_whitespace().map(bare_id).filter(|id| !id.is_empty()).collect()
}

/// `Ann <ann@x.test>` or `ann@x.test`.
pub fn person_from_text(text: &str) -> Person {
    let text = text.trim();
    if let (Some(open), Some(close)) = (text.rfind('<'), text.rfind('>'))
        && open < close
    {
        return Person {
            name: text[..open].trim().trim_matches('"').trim().to_string(),
            address: text[open + 1..close].trim().to_ascii_lowercase(),
        };
    }
    Person { name: String::new(), address: text.to_ascii_lowercase() }
}

fn person(value: &Value) -> Option<Person> {
    match value {
        Value::String(text) => Some(person_from_text(text)).filter(|p| !p.address.is_empty()),
        Value::Object(map) => {
            let address = map.get("address").or_else(|| map.get("Email")).and_then(Value::as_str)?.trim().to_ascii_lowercase();
            let name = map.get("name").or_else(|| map.get("Name")).and_then(Value::as_str).unwrap_or_default().trim().to_string();
            Some(Person { name, address }).filter(|p| !p.address.is_empty())
        }
        _ => None,
    }
}

fn people(value: Option<&Value>) -> Vec<Person> {
    match value {
        Some(Value::Array(list)) => list.iter().filter_map(person).collect(),
        Some(other) => person(other).into_iter().collect(),
        None => Vec::new(),
    }
}

fn decode(text: &str) -> Result<Vec<u8>, ParseError> {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD
        .decode(compact.as_bytes())
        .map_err(|_| bad("an attachment is not valid base64"))
}

/// Why a message is not from a person, from the headers that say so (`name -> value`, names lower-cased).
pub fn automatic_reason(header: impl Fn(&str) -> Option<String>, from: &str) -> Option<String> {
    if let Some(value) = header("auto-submitted").filter(|v| !v.trim().eq_ignore_ascii_case("no")) {
        return Some(format!("Auto-Submitted: {}", value.trim()));
    }
    if let Some(value) = header("precedence").filter(|v| ["bulk", "list", "junk", "auto_reply"].contains(&v.trim().to_ascii_lowercase().as_str())) {
        return Some(format!("Precedence: {}", value.trim()));
    }
    if header("list-id").is_some() || header("list-unsubscribe").is_some() {
        return Some("mailing list".into());
    }
    if header("x-autoreply").is_some() || header("x-autorespond").is_some() {
        return Some("auto-reply".into());
    }
    let local = from.split('@').next().unwrap_or_default();
    if ["mailer-daemon", "postmaster", "noreply", "no-reply", "donotreply", "do-not-reply"].contains(&local) {
        return Some("sent from a system address".into());
    }
    None
}

fn auth_from(header: impl Fn(&str) -> Option<String>) -> Auth {
    let word = |text: &str, key: &str| {
        let at = text.to_ascii_lowercase().find(&format!("{key}="))?;
        let rest = &text[at + key.len() + 1..];
        Some(rest.split(|c: char| !c.is_ascii_alphabetic()).next().unwrap_or_default().to_ascii_lowercase())
    };
    let results = header("authentication-results").unwrap_or_default();
    let spf = word(&results, "spf")
        .or_else(|| header("received-spf").map(|v| v.split_whitespace().next().unwrap_or_default().to_ascii_lowercase()))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "none".into());
    let dkim = word(&results, "dkim").filter(|v| !v.is_empty()).unwrap_or_else(|| "none".into());
    Auth { spf, dkim }
}

fn postmark(body: &Value) -> Result<Inbound, ParseError> {
    let headers: Vec<(String, String)> = body
        .get("Headers")
        .and_then(Value::as_array)
        .map(|list| list.iter().map(|h| (text(h, "Name").to_ascii_lowercase(), text(h, "Value"))).collect())
        .unwrap_or_default();
    let header = |name: &str| headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone());
    let from = body
        .get("FromFull")
        .and_then(person)
        .or_else(|| body.get("From").and_then(person))
        .ok_or_else(|| bad("the message has no sender address"))?;
    let mut message_id = bare_id(&text(body, "MessageID"));
    if let Some(real) = header("message-id") {
        message_id = bare_id(&real);
    }
    let mut attachments = Vec::new();
    for item in body.get("Attachments").and_then(Value::as_array).into_iter().flatten() {
        attachments.push(RawAttachment {
            name: text(item, "Name"),
            content_type: text(item, "ContentType"),
            bytes: decode(&text(item, "Content"))?,
        });
    }
    let mut to = people(body.get("ToFull"));
    if to.is_empty() && let Some(original) = body.get("OriginalRecipient").and_then(person) {
        to.push(original);
    }
    Ok(Inbound {
        automatic: automatic_reason(&header, &from.address),
        auth: auth_from(&header),
        from,
        to,
        cc: people(body.get("CcFull")),
        subject: text(body, "Subject"),
        text: body.get("TextBody").and_then(Value::as_str).unwrap_or_default().to_string(),
        html: body.get("HtmlBody").and_then(Value::as_str).unwrap_or_default().to_string(),
        message_id,
        in_reply_to: header("in-reply-to").map(|v| bare_id(&v)).unwrap_or_default(),
        references: header("references").map(|v| ids(&v)).unwrap_or_default(),
        received_at: text(body, "Date"),
        attachments,
        dropped_attachments: Vec::new(),
    })
}

/// The shape of our own documentation: any provider or script can post it.
fn generic(body: &Value) -> Result<Inbound, ParseError> {
    let headers: Map<String, Value> = body
        .get("headers")
        .and_then(Value::as_object)
        .map(|m| m.iter().map(|(k, v)| (k.to_ascii_lowercase(), v.clone())).collect())
        .unwrap_or_default();
    let header = |name: &str| headers.get(name).and_then(Value::as_str).map(str::to_string);
    let from = body.get("from").and_then(person).ok_or_else(|| bad("the message has no sender address"))?;
    let mut attachments = Vec::new();
    for item in body.get("attachments").and_then(Value::as_array).into_iter().flatten() {
        attachments.push(RawAttachment {
            name: text(item, "name"),
            content_type: text(item, "type"),
            bytes: decode(&text(item, "content_base64"))?,
        });
    }
    let auth = body.get("auth").map_or_else(
        || auth_from(&header),
        |a| Auth {
            spf: text(a, "spf").to_ascii_lowercase().chars().take(16).collect::<String>().if_empty("none"),
            dkim: text(a, "dkim").to_ascii_lowercase().chars().take(16).collect::<String>().if_empty("none"),
        },
    );
    Ok(Inbound {
        automatic: automatic_reason(&header, &from.address),
        auth,
        from,
        to: people(body.get("to")),
        cc: people(body.get("cc")),
        subject: text(body, "subject"),
        text: body.get("text").and_then(Value::as_str).unwrap_or_default().to_string(),
        html: body.get("html").and_then(Value::as_str).unwrap_or_default().to_string(),
        message_id: bare_id(&text(body, "message_id")),
        in_reply_to: bare_id(&text(body, "in_reply_to")),
        references: body
            .get("references")
            .and_then(Value::as_array)
            .map(|l| l.iter().filter_map(Value::as_str).map(bare_id).collect())
            .unwrap_or_default(),
        received_at: text(body, "received_at"),
        attachments,
        dropped_attachments: Vec::new(),
    })
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() { fallback.to_string() } else { self }
    }
}

impl Inbound {
    /// What a plugin function receives, with the attachments already stored (`stored`).
    pub fn to_json(&self, stored: Vec<Value>) -> Value {
        json!({
            "from": { "name": self.from.name, "address": self.from.address },
            "to": self.to,
            "cc": self.cc,
            "subject": self.subject,
            "text": self.text,
            "html_sanitized": self.html,
            "attachments": stored,
            "dropped_attachments": self.dropped_attachments,
            "message_id": self.message_id,
            "in_reply_to": self.in_reply_to,
            "references": self.references,
            "received_at": self.received_at,
            "auth": self.auth,
        })
    }

    /// Every address the message was sent to, lower-cased.
    pub fn recipients(&self) -> impl Iterator<Item = &str> {
        self.to.iter().chain(self.cc.iter()).map(|p| p.address.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn postmark_body() -> Value {
        json!({
            "FromFull": { "Email": "Ann@Client.test", "Name": "Ann" },
            "ToFull": [{ "Email": "support+tk_abc123@mail.acme.test", "Name": "" }],
            "Subject": "Printer",
            "MessageID": "uuid-1",
            "TextBody": "It is on fire",
            "HtmlBody": "<p onclick=\"x()\">It is <b>on fire</b></p><script>alert(1)</script><a href=\"javascript:x()\">hi</a>",
            "Headers": [
                { "Name": "Message-ID", "Value": "<Real-1@client.test>" },
                { "Name": "In-Reply-To", "Value": "<tk_abc123.1@mail.acme.test>" },
                { "Name": "References", "Value": "<a@x> <tk_abc123.1@mail.acme.test>" },
                { "Name": "Received-SPF", "Value": "Pass (sender ok)" },
                { "Name": "Authentication-Results", "Value": "mx; dkim=pass header.d=client.test" }
            ],
            "Attachments": [{ "Name": "a.txt", "ContentType": "text/plain", "Content": "aGVsbG8=" }]
        })
    }

    #[test]
    fn postmark_mail_becomes_the_common_shape() {
        let m = parse("postmark", &postmark_body()).unwrap();
        assert_eq!(m.from, Person { name: "Ann".into(), address: "ann@client.test".into() });
        assert_eq!(m.message_id, "real-1@client.test", "the real Message-ID, not the provider's");
        assert_eq!(m.in_reply_to, "tk_abc123.1@mail.acme.test");
        assert_eq!(m.references.len(), 2);
        assert_eq!((m.auth.spf.as_str(), m.auth.dkim.as_str()), ("pass", "pass"));
        assert_eq!(m.attachments[0].bytes, b"hello");
        assert_eq!(m.automatic, None);
        assert_eq!(m.recipients().collect::<Vec<_>>(), vec!["support+tk_abc123@mail.acme.test"]);
    }

    #[test]
    fn html_is_cleaned_before_anyone_sees_it() {
        let m = parse("postmark", &postmark_body()).unwrap();
        assert!(m.html.contains("<b>on fire</b>"));
        for danger in ["script", "onclick", "javascript:", "alert"] {
            assert!(!m.html.contains(danger), "{danger} survived: {}", m.html);
        }
    }

    #[test]
    fn mail_nobody_wrote_is_recognised() {
        let h = |pairs: &'static [(&str, &str)]| move |n: &str| pairs.iter().find(|(k, _)| *k == n).map(|(_, v)| v.to_string());
        assert!(automatic_reason(h(&[("auto-submitted", "auto-replied")]), "a@b.test").is_some());
        assert!(automatic_reason(h(&[("auto-submitted", "no")]), "a@b.test").is_none());
        assert!(automatic_reason(h(&[("precedence", "bulk")]), "a@b.test").is_some());
        assert!(automatic_reason(h(&[("list-id", "<x.y>")]), "a@b.test").is_some());
        assert!(automatic_reason(h(&[]), "mailer-daemon@b.test").is_some());
        assert!(automatic_reason(h(&[]), "ann@b.test").is_none());
    }

    #[test]
    fn the_generic_shape_takes_names_or_strings() {
        let body = json!({
            "from": "Bob <BOB@x.test>", "to": ["support@mail.acme.test"], "subject": "Hi", "message_id": "<M1@x.test>",
            "text": "hello", "auth": { "spf": "PASS", "dkim": "" },
            "attachments": [{ "name": "n.txt", "type": "text/plain", "content_base64": "aGk=" }]
        });
        let m = parse("generic", &body).unwrap();
        assert_eq!(m.from.address, "bob@x.test");
        assert_eq!(m.from.name, "Bob");
        assert_eq!(m.message_id, "m1@x.test");
        assert_eq!((m.auth.spf.as_str(), m.auth.dkim.as_str()), ("pass", "none"));
    }

    #[test]
    fn what_cannot_be_used_is_refused() {
        assert!(parse("generic", &json!({ "message_id": "x", "from": "nobody" })).is_err(), "no address");
        assert!(parse("generic", &json!({ "from": "a@b.test" })).is_err(), "no Message-ID");
        assert!(parse("nosuch", &json!({})).is_err());
        let mut body = postmark_body();
        body["Attachments"][0]["Content"] = json!("!!!");
        assert!(parse("postmark", &body).is_err(), "bad base64");
    }

    #[test]
    fn too_many_attachments_are_named_not_kept() {
        let many: Vec<Value> = (0..MAX_ATTACHMENTS + 3).map(|i| json!({ "name": format!("f{i}"), "type": "text/plain", "content_base64": "aGk=" })).collect();
        let m = parse("generic", &json!({ "from": "a@b.test", "message_id": "x", "attachments": many })).unwrap();
        assert_eq!((m.attachments.len(), m.dropped_attachments.len()), (MAX_ATTACHMENTS, 3));
    }
}
