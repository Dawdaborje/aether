//! A person's own mailbox on any IMAP server, for mail servers that do not speak JMAP (most of them: Dovecot, Courier, Exchange, Gmail).
//!
//! The same actions as the `jmap` bridge, so a plugin can use either: `check`, `mailboxes`, `list`, `get`, `attachment`, `mark`,
//! `trash`, `send`. Reading goes over IMAP (`host`, TLS by default); **sending goes over SMTP** (`smtp_host`) with the person's own login,
//! and a copy is put in the Sent folder. The login is the person's own and not a setting: the kernel keeps it per person and hands it
//! over as `_credential`. A message's id is `<uid>.<folder>` and a folder's id is its name in hexadecimal, so ids hold only letters and digits.

mod client;
mod mime;

use std::time::Duration;

use aether_communication::{Action, ActionBridge, ConfigError, Field, REQUEST_TIMEOUT_SECS, SendError, Spec, Values};
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD};
use client::{Client, Part, Security, V, after, quote};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Attachment, Mailbox, MultiPart, SinglePart, header::ContentType},
    transport::smtp::authentication::Credentials,
};
use serde_json::{Value, json};

pub const SPEC: Spec = Spec {
    key: "imap",
    label: "Mailbox (IMAP)",
    account: &[],
    options: &[
        Field { name: "host", label: "IMAP server", secret: false, required: true, default: None },
        Field { name: "port", label: "IMAP port (993 for tls, 143 for starttls)", secret: false, required: false, default: None },
        Field { name: "security", label: "IMAP security (tls, starttls or none)", secret: false, required: false, default: Some("tls") },
        Field { name: "smtp_host", label: "SMTP server (for sending)", secret: false, required: true, default: None },
        Field { name: "smtp_port", label: "SMTP port (587 for starttls, 465 for tls, 25 for none)", secret: false, required: false, default: None },
        Field { name: "smtp_security", label: "SMTP security (starttls, tls or none)", secret: false, required: false, default: Some("starttls") },
        Field { name: "domain", label: "Domain for logins that are not an address (ann becomes ann@domain)", secret: false, required: false, default: None },
    ],
};

pub const ACTIONS: &[Action] = &[
    Action { name: "check", help: "{} -> { name, account } (is the login accepted; the kernel calls this before keeping a login)" },
    Action { name: "mailboxes", help: "{} -> [{ id, name, role, total, unread }]" },
    Action { name: "list", help: "{ mailbox? (id; default the inbox), search?, limit? (1 to 50, default 25), position? } -> { messages: [{ id, from, subject, preview, received_at, unread, attachments }], total }" },
    Action { name: "get", help: "{ id } -> { id, from, to, cc, subject, received_at, text, message_id, references, unread, attachments: [{ name, type, size }] }" },
    Action { name: "attachment", help: "{ id (message), index (position in the get answer's attachments) } -> { name, type, size, base64 } (at most 10 MB)" },
    Action { name: "mark", help: "{ id, read } -> { ok }" },
    Action { name: "trash", help: "{ id } -> { ok } (moves it to the trash)" },
    Action { name: "send", help: "{ to: [address], cc?: [address], subject, text, in_reply_to?, references?, attachments?: [{ name, type, base64 }] (at most 10 MB in all) } -> { id }" },
];

const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;

pub struct Imap {
    host: String,
    port: u16,
    security: Security,
    smtp_host: String,
    smtp_port: u16,
    smtp_security: Security,
    domain: String,
}

impl Imap {
    pub fn new(values: &Values) -> Result<Self, ConfigError> {
        let get = |name: &str| values.get(name).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let security = |name: &str, default: &str| match get(name).unwrap_or_else(|| default.to_string()).as_str() {
            "tls" => Ok(Security::Tls),
            "starttls" => Ok(Security::StartTls),
            "none" => Ok(Security::None),
            other => Err(ConfigError(format!("imap: {name} `{other}` is not tls, starttls or none"))),
        };
        let port = |name: &str, default: u16| match get(name) {
            Some(p) => p.parse().map_err(|_| ConfigError(format!("imap: `{p}` is not a port number"))),
            None => Ok(default),
        };
        let (sec, smtp_sec) = (security("security", "tls")?, security("smtp_security", "starttls")?);
        Ok(Self {
            host: get("host").unwrap_or_default(),
            port: port("port", match sec { Security::Tls => 993, _ => 143 })?,
            security: sec,
            smtp_host: get("smtp_host").unwrap_or_default(),
            smtp_port: port("smtp_port", match smtp_sec { Security::StartTls => 587, Security::Tls => 465, Security::None => 25 })?,
            smtp_security: smtp_sec,
            domain: get("domain").unwrap_or_default(),
        })
    }
}

struct Login {
    username: String,
    secret: String,
}

impl Imap {
    async fn open(&self, login: &Login) -> Result<Client, SendError> {
        if self.host.is_empty() {
            return Err(SendError::permanent("imap: `host` is not set in the bridge settings"));
        }
        let mut client = Client::connect(&self.host, self.port, self.security).await?;
        client.login(&login.username, &login.secret).await?;
        Ok(client)
    }

    /// The address mail is sent from: the login when it is an address, else the login at the configured domain.
    fn address(&self, login: &Login) -> Result<String, SendError> {
        if login.username.contains('@') {
            Ok(login.username.clone())
        } else if !self.domain.is_empty() {
            Ok(format!("{}@{}", login.username, self.domain))
        } else {
            Err(SendError::permanent("this login is not an address and the administrator has not set the domain for it"))
        }
    }
}

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<String> {
    if s.is_empty() || s.len() % 2 != 0 || s.len() > 1000 {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect();
    String::from_utf8(bytes?).ok().filter(|n| !n.contains(['\r', '\n', '\0']))
}

/// A folder name as the server writes it (modified UTF-7, RFC 3501) as text.
fn folder_text(name: &str) -> String {
    let mut out = String::new();
    let mut rest = name;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let Some(end) = tail.find('-') else {
            out.push_str(&rest[start..]);
            return out;
        };
        if end == 0 {
            out.push('&');
        } else {
            let b64 = tail[..end].replace(',', "/");
            let bytes = STANDARD.decode(format!("{b64}{}", "=".repeat((4 - b64.len() % 4) % 4))).unwrap_or_default();
            let units: Vec<u16> = bytes.chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
            out.push_str(&String::from_utf16_lossy(&units));
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

struct Folder {
    name: String,
    role: Option<&'static str>,
}

fn role_of(name: &str, attrs: &[V]) -> Option<&'static str> {
    if name.eq_ignore_ascii_case("INBOX") {
        return Some("inbox");
    }
    for (flag, role) in [("\\Sent", "sent"), ("\\Drafts", "drafts"), ("\\Trash", "trash"), ("\\Junk", "junk"), ("\\Archive", "archive")] {
        if attrs.iter().any(|a| a.is(flag)) {
            return Some(role);
        }
    }
    match folder_text(name).to_lowercase().rsplit(['/', '.']).next().unwrap_or_default() {
        "sent" | "sent items" | "sent mail" => Some("sent"),
        "drafts" => Some("drafts"),
        "trash" | "deleted items" | "deleted messages" => Some("trash"),
        "junk" | "spam" => Some("junk"),
        _ => None,
    }
}

async fn folders(c: &mut Client) -> Result<Vec<Folder>, SendError> {
    let mut out = Vec::new();
    for item in c.cmd("LIST \"\" \"*\"").await? {
        if !item.first().is_some_and(|v| v.is("LIST")) || item.len() < 4 {
            continue;
        }
        let attrs = item[1].list();
        if attrs.iter().any(|a| a.is("\\Noselect") || a.is("\\NonExistent")) {
            continue;
        }
        let name = item[3].text();
        if name.is_empty() || name.contains(['\r', '\n']) {
            continue;
        }
        out.push(Folder { role: role_of(&name, attrs), name });
    }
    Ok(out)
}

fn folder_named(params: &Value, list: &[Folder], role: &str) -> Option<String> {
    let _ = params;
    list.iter().find(|f| f.role == Some(role)).map(|f| f.name.clone())
}

/// `<uid>.<hex folder>` as (uid, folder).
fn message_ref(params: &Value) -> Result<(u32, String), SendError> {
    let id = params["id"].as_str().unwrap_or_default();
    let bad = || SendError::permanent("`id` is required: a message id");
    let (uid, folder) = id.split_once('.').ok_or_else(bad)?;
    Ok((uid.parse().map_err(|_| bad())?, unhex(folder).ok_or_else(bad)?))
}

fn rfc3339(internal: &str) -> String {
    chrono::DateTime::parse_from_str(internal.trim(), "%d-%b-%Y %H:%M:%S %z")
        .map(|d| d.to_utc().to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

fn has_word(v: &V, word: &str) -> bool {
    match v {
        V::List(l) => l.iter().any(|x| has_word(x, word)),
        V::Atom(_) | V::Str(_) => v.text().eq_ignore_ascii_case(word),
        V::Nil => false,
    }
}

fn from_line(a: &[mime::Address]) -> String {
    a.first().map(|p| if p.name.is_empty() { p.email.clone() } else { format!("{} <{}>", p.name, p.email) }).unwrap_or_default()
}

fn people(list: &[mime::Address]) -> Vec<Value> {
    list.iter().map(|p| json!({ "name": p.name, "email": p.email })).collect()
}

/// The fetched message `uid` (all of it): its data items and the parsed message.
async fn fetch_message(c: &mut Client, folder: &str, uid: u32) -> Result<(Vec<V>, mime::Parsed), SendError> {
    c.cmd(&format!("EXAMINE {}", quote(folder))).await?;
    let found = c.cmd(&format!("UID FETCH {uid} (UID FLAGS INTERNALDATE BODY.PEEK[])")).await?;
    let items = found
        .into_iter()
        .find(|r| r.get(1).is_some_and(|v| v.is("FETCH")) && r.get(2).is_some_and(|l| after(l.list(), "UID").is_some_and(|u| u.text() == uid.to_string())))
        .and_then(|r| r.into_iter().nth(2))
        .ok_or_else(|| SendError::permanent("there is no such message"))?;
    let items = items.list().to_vec();
    let raw = items.iter().position(|v| matches!(v, V::Atom(a) if a.to_ascii_uppercase().starts_with("BODY["))).and_then(|i| items.get(i + 1)).map(|v| v.bytes().to_vec()).unwrap_or_default();
    Ok((items, mime::parse(&raw)))
}

fn unread(items: &[V]) -> bool {
    !after(items, "FLAGS").is_some_and(|f| f.list().iter().any(|x| x.is("\\Seen")))
}

fn addresses(params: &Value, name: &str) -> Result<Vec<String>, SendError> {
    let mut out = Vec::new();
    for a in params[name].as_array().cloned().unwrap_or_default() {
        let a = a.as_str().unwrap_or_default().trim().to_string();
        if a.is_empty() || a.len() > 320 || !a.contains('@') || a.contains(['\r', '\n', '<', '>', ',', ' ']) {
            return Err(SendError::permanent(format!("`{name}` holds an address that is not valid: {a}")));
        }
        out.push(a);
    }
    Ok(out)
}

fn ids_header(params: &Value, name: &str) -> String {
    let text = |v: &Value| v.as_str().map(|s| s.trim().trim_matches(['<', '>']).to_string()).filter(|s| !s.is_empty() && !s.contains(['\r', '\n', ' ', '<', '>']));
    let list: Vec<String> = match &params[name] {
        Value::String(s) => s.split_whitespace().filter_map(|p| text(&json!(p))).collect(),
        Value::Array(a) => a.iter().filter_map(text).collect(),
        _ => Vec::new(),
    };
    list.iter().map(|i| format!("<{i}>")).collect::<Vec<_>>().join(" ")
}

fn mailbox(address: &str) -> Result<Mailbox, SendError> {
    Ok(Mailbox::new(None, address.parse().map_err(|_| SendError::permanent(format!("`{address}` is not an email address")))?))
}

/// The message as it goes on the wire.
fn build_message(from: &str, params: &Value) -> Result<Message, SendError> {
    let to = addresses(params, "to")?;
    let cc = addresses(params, "cc")?;
    if to.is_empty() || to.len() + cc.len() > 50 {
        return Err(SendError::permanent("`to` needs at least one address (50 in all)"));
    }
    let subject = params["subject"].as_str().map(str::trim).unwrap_or_default();
    if subject.is_empty() || subject.len() > 300 || subject.contains(['\r', '\n']) {
        return Err(SendError::permanent("`subject` is required, one line, at most 300 characters"));
    }
    let text = params["text"].as_str().unwrap_or_default();
    if text.trim().is_empty() || text.len() > 200_000 {
        return Err(SendError::permanent("`text` is required (at most 200,000 characters)"));
    }
    let mut builder = Message::builder().from(mailbox(from)?).subject(subject);
    for a in &to {
        builder = builder.to(mailbox(a)?);
    }
    for a in &cc {
        builder = builder.cc(mailbox(a)?);
    }
    let (reply, refs) = (ids_header(params, "in_reply_to"), ids_header(params, "references"));
    if !reply.is_empty() {
        builder = builder.in_reply_to(reply);
    }
    if !refs.is_empty() {
        builder = builder.references(refs);
    }
    let mut total = 0usize;
    let mut files = Vec::new();
    for item in params["attachments"].as_array().into_iter().flatten().take(20) {
        let bytes = STANDARD.decode(item["base64"].as_str().unwrap_or_default()).map_err(|_| SendError::permanent("an attachment is not valid base64"))?;
        total += bytes.len();
        if bytes.is_empty() || total > MAX_ATTACHMENT_BYTES {
            return Err(SendError::permanent("attachments are at most 10 MB in all, none empty"));
        }
        let kind = ContentType::parse(item["type"].as_str().unwrap_or_default()).unwrap_or_else(|_| ContentType::parse("application/octet-stream").expect("a valid type"));
        let name = item["name"].as_str().unwrap_or("attachment").replace(['\r', '\n', '"', '/', '\\'], "_");
        files.push(Attachment::new(name).body(bytes, kind));
    }
    let built = if files.is_empty() {
        builder.header(ContentType::TEXT_PLAIN).body(text.to_string())
    } else {
        let mut parts = MultiPart::mixed().singlepart(SinglePart::plain(text.to_string()));
        for f in files {
            parts = parts.singlepart(f);
        }
        builder.multipart(parts)
    };
    built.map_err(|e| SendError::permanent(format!("the message could not be built: {e}")))
}

#[async_trait]
impl ActionBridge for Imap {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, SendError> {
        if !ACTIONS.iter().any(|a| a.name == action) {
            return Err(SendError::permanent(format!("imap has no action `{action}`; it offers: {}", ACTIONS.iter().map(|a| a.name).collect::<Vec<_>>().join(", "))));
        }
        let login = Login {
            username: params["_credential"]["username"].as_str().unwrap_or_default().to_string(),
            secret: params["_credential"]["secret"].as_str().unwrap_or_default().to_string(),
        };
        if login.username.is_empty() {
            return Err(SendError::permanent("you have not connected your mailbox yet"));
        }
        if action == "send" {
            return self.send(&login, params).await;
        }
        let mut c = self.open(&login).await?;
        let answer = self.act(&mut c, action, params, &login).await;
        c.logout().await;
        answer
    }
}

impl Imap {
    async fn act(&self, c: &mut Client, action: &str, params: &Value, login: &Login) -> Result<Value, SendError> {
        match action {
            "check" => Ok(json!({ "name": login.username, "account": login.username })),
            "mailboxes" => {
                let mut rows = Vec::new();
                for f in folders(c).await?.into_iter().take(60) {
                    let status = c.cmd(&format!("STATUS {} (MESSAGES UNSEEN)", quote(&f.name))).await.unwrap_or_default();
                    let counts = status.iter().find(|r| r.first().is_some_and(|v| v.is("STATUS"))).and_then(|r| r.get(2)).map(|l| l.list().to_vec()).unwrap_or_default();
                    let count = |k: &str| after(&counts, k).and_then(|v| v.text().parse::<u64>().ok()).unwrap_or(0);
                    let order = match f.role { Some("inbox") => 0, Some("drafts") => 1, Some("sent") => 2, Some("archive") => 3, Some("junk") => 4, Some("trash") => 5, _ => 10 };
                    rows.push((order, folder_text(&f.name), json!({ "id": hex(&f.name), "name": folder_text(&f.name), "role": f.role, "total": count("MESSAGES"), "unread": count("UNSEEN"), "sortOrder": order })));
                }
                rows.sort_by(|a, b| (a.0, a.1.to_lowercase()).cmp(&(b.0, b.1.to_lowercase())));
                Ok(Value::Array(rows.into_iter().map(|r| r.2).collect()))
            }
            "list" => {
                let limit = params["limit"].as_u64().unwrap_or(25);
                if !(1..=50).contains(&limit) {
                    return Err(SendError::permanent("`limit` is 1 to 50"));
                }
                let folder = match params["mailbox"].as_str().filter(|m| !m.is_empty()) {
                    Some(id) => unhex(id).ok_or_else(|| SendError::permanent("`mailbox` is a folder id"))?,
                    None => "INBOX".to_string(),
                };
                c.cmd(&format!("EXAMINE {}", quote(&folder))).await?;
                let found = match params["search"].as_str().map(str::trim).filter(|t| !t.is_empty()) {
                    Some(text) => {
                        let text: String = text.chars().take(200).collect();
                        c.run(&[Part::Text("UID SEARCH CHARSET UTF-8 TEXT ".into()), Part::Lit(text.into_bytes())]).await?
                    }
                    None => c.cmd("UID SEARCH ALL").await?,
                };
                let mut uids: Vec<u32> = found.iter().filter(|r| r.first().is_some_and(|v| v.is("SEARCH"))).flat_map(|r| r.iter().skip(1)).filter_map(|v| v.text().parse().ok()).collect();
                uids.sort_unstable_by(|a, b| b.cmp(a));
                let total = uids.len();
                let position = params["position"].as_u64().unwrap_or(0) as usize;
                let page: Vec<u32> = uids.into_iter().skip(position).take(limit as usize).collect();
                let mut messages = Vec::new();
                if !page.is_empty() {
                    let set = page.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
                    let fetched = c.cmd(&format!("UID FETCH {set} (UID FLAGS INTERNALDATE BODYSTRUCTURE BODY.PEEK[HEADER.FIELDS (FROM SUBJECT)])")).await?;
                    for uid in &page {
                        let Some(items) = fetched
                            .iter()
                            .filter(|r| r.get(1).is_some_and(|v| v.is("FETCH")))
                            .filter_map(|r| r.get(2))
                            .map(|l| l.list())
                            .find(|l| after(l, "UID").is_some_and(|u| u.text() == uid.to_string()))
                        else {
                            continue;
                        };
                        let head = items.iter().position(|v| matches!(v, V::Atom(a) if a.to_ascii_uppercase().starts_with("BODY[HEADER"))).and_then(|i| items.get(i + 1)).map(|v| v.bytes().to_vec()).unwrap_or_default();
                        let parsed = mime::parse(&head);
                        let subject = parsed.subject();
                        messages.push(json!({
                            "id": format!("{uid}.{}", hex(&folder)), "from": from_line(&parsed.addresses("from")),
                            "subject": if subject.is_empty() { "(no subject)".to_string() } else { subject }, "preview": "",
                            "received_at": rfc3339(&after(items, "INTERNALDATE").map(V::text).unwrap_or_default()), "unread": unread(items),
                            "attachments": after(items, "BODYSTRUCTURE").is_some_and(|b| has_word(b, "attachment")),
                        }));
                    }
                }
                Ok(json!({ "messages": messages, "total": total }))
            }
            "get" => {
                let (uid, folder) = message_ref(params)?;
                let (items, m) = fetch_message(c, &folder, uid).await?;
                let first_id = |name: &str| m.header(name).map(mime::message_ids).and_then(|l| l.into_iter().next());
                Ok(json!({
                    "id": params["id"], "from": people(&m.addresses("from")), "to": people(&m.addresses("to")), "cc": people(&m.addresses("cc")),
                    "subject": if m.subject().is_empty() { "(no subject)".to_string() } else { m.subject() },
                    "received_at": rfc3339(&after(&items, "INTERNALDATE").map(V::text).unwrap_or_default()), "text": m.text,
                    "message_id": first_id("message-id"), "references": m.header("references").map(mime::message_ids), "unread": unread(&items),
                    "attachments": m.attachments.iter().map(|a| json!({ "name": a.name, "type": a.kind, "size": a.data.len() })).collect::<Vec<_>>(),
                }))
            }
            "attachment" => {
                let (uid, folder) = message_ref(params)?;
                let index = params["index"].as_u64().unwrap_or(u64::MAX) as usize;
                let (_, m) = fetch_message(c, &folder, uid).await?;
                let a = m.attachments.get(index).ok_or_else(|| SendError::permanent("the message has no such attachment"))?;
                if a.data.len() > MAX_ATTACHMENT_BYTES {
                    return Err(SendError::permanent("that attachment is larger than 10 MB"));
                }
                Ok(json!({ "name": a.name, "type": a.kind, "size": a.data.len(), "base64": STANDARD.encode(&a.data) }))
            }
            "mark" => {
                let (uid, folder) = message_ref(params)?;
                c.cmd(&format!("SELECT {}", quote(&folder))).await?;
                let sign = if params["read"].as_bool().unwrap_or(true) { '+' } else { '-' };
                c.cmd(&format!("UID STORE {uid} {sign}FLAGS.SILENT (\\Seen)")).await?;
                Ok(json!({ "ok": true }))
            }
            "trash" => {
                let (uid, folder) = message_ref(params)?;
                let list = folders(c).await?;
                let trash = folder_named(params, &list, "trash").ok_or_else(|| SendError::permanent("this mailbox has no trash folder"))?;
                if trash == folder {
                    return Ok(json!({ "ok": true }));
                }
                c.cmd(&format!("SELECT {}", quote(&folder))).await?;
                let caps: Vec<String> = c.cmd("CAPABILITY").await?.iter().flat_map(|r| r.iter().map(|v| v.text().to_ascii_uppercase())).collect();
                if caps.iter().any(|x| x == "MOVE") {
                    c.cmd(&format!("UID MOVE {uid} {}", quote(&trash))).await?;
                } else {
                    c.cmd(&format!("UID COPY {uid} {}", quote(&trash))).await?;
                    c.cmd(&format!("UID STORE {uid} +FLAGS.SILENT (\\Deleted)")).await?;
                    if caps.iter().any(|x| x == "UIDPLUS") {
                        c.cmd(&format!("UID EXPUNGE {uid}")).await?;
                    }
                }
                Ok(json!({ "ok": true }))
            }
            _ => unreachable!("checked above"),
        }
    }

    /// Send over SMTP with the person's own login, then keep a copy in the Sent folder (best effort).
    async fn send(&self, login: &Login, params: &Value) -> Result<Value, SendError> {
        if self.smtp_host.is_empty() {
            return Err(SendError::permanent("imap: `smtp_host` is not set in the bridge settings"));
        }
        let message = build_message(&self.address(login)?, params)?;
        let wire = message.formatted();
        let builder = match self.smtp_security {
            Security::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.smtp_host),
            Security::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&self.smtp_host),
            Security::None => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.smtp_host)),
        }
        .map_err(|e| SendError::permanent(format!("the SMTP server `{}` cannot be used: {e}", self.smtp_host)))?;
        let transport = builder
            .port(self.smtp_port)
            .timeout(Some(Duration::from_secs(REQUEST_TIMEOUT_SECS)))
            .credentials(Credentials::new(login.username.clone(), login.secret.clone()))
            .build();
        match transport.send(message).await {
            Ok(_) => {}
            Err(error) if error.is_permanent() => return Err(SendError::permanent(format!("the mail server would not send it: {error}"))),
            Err(error) => return Err(SendError::transient(format!("the mail server could not take it: {error}"))),
        }
        if let Ok(mut c) = self.open(login).await {
            if let Ok(list) = folders(&mut c).await {
                if let Some(sent) = folder_named(params, &list, "sent") {
                    let _ = c.run(&[Part::Text(format!("APPEND {} (\\Seen) ", quote(&sent))), Part::Lit(wire)]).await;
                }
            }
            c.logout().await;
        }
        Ok(json!({ "id": "sent" }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(pairs: &[(&str, &str)]) -> Values {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn ids_hold_only_letters_and_digits_and_come_back() {
        let id = format!("7.{}", hex("Archive/2026 é"));
        assert!(id.chars().all(|c| c.is_ascii_alphanumeric() || c == '.'));
        let (uid, folder) = message_ref(&json!({ "id": id })).unwrap();
        assert_eq!((uid, folder.as_str()), (7, "Archive/2026 é"));
        assert!(message_ref(&json!({ "id": "x.zz" })).is_err());
    }

    #[test]
    fn folder_names_are_read_from_modified_utf7() {
        assert_eq!(folder_text("INBOX"), "INBOX");
        assert_eq!(folder_text("Entw&APw-rfe"), "Entwürfe");
        assert_eq!(folder_text("A&-B"), "A&B");
    }

    #[test]
    fn dates_become_utc() {
        assert_eq!(rfc3339("19-Oct-2026 12:30:00 +0200"), "2026-10-19T10:30:00Z");
        assert_eq!(rfc3339(" 5-Oct-2026 01:00:00 +0000"), "2026-10-05T01:00:00Z");
    }

    #[test]
    fn settings_are_checked_and_ports_default_by_security() {
        let base = [("host", "imap.test"), ("smtp_host", "smtp.test")];
        let imap = Imap::new(&values(&base)).unwrap();
        assert_eq!((imap.port, imap.smtp_port), (993, 587));
        let mut plain = base.to_vec();
        plain.extend([("security", "none"), ("smtp_security", "none")]);
        let imap = Imap::new(&values(&plain)).unwrap();
        assert_eq!((imap.port, imap.smtp_port), (143, 25));
        assert!(Imap::new(&values(&[])).is_ok(), "an unset server is reported when it is used, not when the bridge is built");
        assert!(Imap::new(&values(&[("host", "x"), ("smtp_host", "y"), ("security", "ssl")])).is_err());
    }

    #[test]
    fn a_message_with_a_file_is_built_and_a_bad_address_refused() {
        let m = build_message("ann@acme.test", &json!({ "to": ["kim@x.test"], "subject": "Hi", "text": "Body", "attachments": [{ "name": "a.txt", "type": "text/plain", "base64": "SGVsbG8=" }], "in_reply_to": ["a@b"] })).unwrap();
        let raw = String::from_utf8(m.formatted()).unwrap();
        assert!(raw.contains("multipart/mixed") && raw.contains("a.txt") && raw.contains("In-Reply-To: <a@b>"), "{raw}");
        assert!(build_message("ann@acme.test", &json!({ "to": ["a@x.test\r\nBcc: e@x.test"], "subject": "x", "text": "y" })).is_err());
    }

    #[test]
    fn a_login_that_is_not_an_address_needs_the_domain() {
        let mut imap = Imap::new(&values(&[("host", "i"), ("smtp_host", "s")])).unwrap();
        let ann = Login { username: "ann".into(), secret: "x".into() };
        assert!(imap.address(&ann).is_err());
        imap.domain = "acme.test".into();
        assert_eq!(imap.address(&ann).unwrap(), "ann@acme.test");
    }
}
