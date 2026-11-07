//! A person's own mailbox on a JMAP mail server (RFC 8620 and 8621): Stalwart, Cyrus, Fastmail and others.
//!
//! The mail server is one setting (`session_url`, such as `https://mail.example.com/.well-known/jmap`). The **login is the
//! person's own** and not a setting: the kernel keeps it per person (see `bridge_credentials` in the core) and hands it to this
//! bridge as `_credential` when that person's plugin calls it, so a plugin only ever reaches the mailbox of whoever is using it.
//!
//! Actions: `check`, `mailboxes`, `list`, `get`, `attachment`, `mark`, `trash`, `send`. Mail text only: the bridge asks for plain text parts
//! (a message that has only HTML comes back with its tags removed) and never returns scripts or remote content.

use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};

use aether_communication::{Action, ActionBridge, ConfigError, Field, REQUEST_TIMEOUT_SECS, SendError, Spec, Values};
use async_trait::async_trait;
use serde_json::{Value, json};

pub const SPEC: Spec = Spec {
    key: "jmap",
    label: "Mailbox (JMAP)",
    account: &[],
    options: &[Field { name: "session_url", label: "JMAP session address of the mail server", secret: false, required: true, default: None }],
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

const USING: &[&str] = &["urn:ietf:params:jmap:core", "urn:ietf:params:jmap:mail", "urn:ietf:params:jmap:submission"];
const MAIL: &str = "urn:ietf:params:jmap:mail";

pub struct Jmap {
    session_url: String,
}

impl Jmap {
    pub fn new(values: &Values) -> Result<Self, ConfigError> {
        Ok(Self { session_url: values.get("session_url").map(|u| u.trim().to_string()).unwrap_or_default() })
    }
}

struct Login {
    username: String,
    secret: String,
}

struct Session {
    api_url: String,
    account: String,
    download_url: String,
    upload_url: String,
}

/// Most bytes of attachments the bridge moves in one call.
const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;

/// Fills `{name}` placeholders of a JMAP URL template; values are percent-encoded.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (name, value) in values {
        let encoded: String = value.bytes().map(|b| if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
        out = out.replace(&format!("{{{name}}}"), &encoded);
    }
    out
}

fn client() -> Result<reqwest::Client, SendError> {
    reqwest::Client::builder().timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS)).build().map_err(|_| SendError::transient("the mail client could not start"))
}

fn network(error: reqwest::Error) -> SendError {
    SendError::transient(format!("the mail server could not be reached ({})", if error.is_timeout() { "timed out" } else { "network error" }))
}

/// What the person sees when the server refuses them, by status.
fn refused(status: u16) -> SendError {
    match status {
        401 | 403 => SendError::permanent("the mail server did not accept your login"),
        _ => SendError::from_status(status, "the mail server refused the request"),
    }
}

impl Jmap {
    async fn session(&self, http: &reqwest::Client, login: &Login) -> Result<Session, SendError> {
        if !self.session_url.starts_with("https://") && !self.session_url.starts_with("http://") {
            return Err(SendError::permanent("jmap: `session_url` is not set in the bridge settings"));
        }
        let response = http.get(&self.session_url).basic_auth(&login.username, Some(&login.secret)).send().await.map_err(network)?;
        if !response.status().is_success() {
            return Err(refused(response.status().as_u16()));
        }
        let body: Value = response.json().await.map_err(|_| SendError::permanent("the mail server's session is not JSON; is the address a JMAP server?"))?;
        let api_url = body["apiUrl"].as_str().ok_or_else(|| SendError::permanent("the mail server gave no API address"))?.to_string();
        let account = body["primaryAccounts"][MAIL].as_str().ok_or_else(|| SendError::permanent("this login has no mail account"))?.to_string();
        let text = |name: &str| body[name].as_str().unwrap_or_default().to_string();
        Ok(Session { api_url, account, download_url: text("downloadUrl"), upload_url: text("uploadUrl") })
    }

    /// One request with several method calls; the answers by call id. A method that failed becomes an error naming it.
    async fn invoke(&self, http: &reqwest::Client, login: &Login, session: &Session, calls: Vec<Value>) -> Result<Vec<(String, Value)>, SendError> {
        let response = http
            .post(&session.api_url)
            .basic_auth(&login.username, Some(&login.secret))
            .json(&json!({ "using": USING, "methodCalls": calls }))
            .send()
            .await
            .map_err(network)?;
        if !response.status().is_success() {
            return Err(refused(response.status().as_u16()));
        }
        let body: Value = response.json().await.map_err(|_| SendError::transient("the mail server sent something that is not JSON"))?;
        let mut out = Vec::new();
        for call in body["methodResponses"].as_array().cloned().unwrap_or_default() {
            let (name, args, id) = (call[0].as_str().unwrap_or_default(), call[1].clone(), call[2].as_str().unwrap_or_default().to_string());
            if name == "error" {
                return Err(SendError::permanent(format!("the mail server says: {}", args["description"].as_str().or(args["type"].as_str()).unwrap_or("error"))));
            }
            out.push((id, args));
        }
        Ok(out)
    }
}

fn answer<'a>(answers: &'a [(String, Value)], id: &str) -> Result<&'a Value, SendError> {
    answers.iter().find(|(i, _)| i == id).map(|(_, v)| v).ok_or_else(|| SendError::permanent("the mail server left out an answer"))
}

fn person(list: &Value) -> Vec<Value> {
    list.as_array().into_iter().flatten().map(|p| json!({ "name": p["name"].as_str().unwrap_or_default(), "email": p["email"].as_str().unwrap_or_default() })).collect()
}

fn from_line(list: &Value) -> String {
    list.as_array()
        .and_then(|l| l.first())
        .map(|p| match (p["name"].as_str().filter(|n| !n.is_empty()), p["email"].as_str()) {
            (Some(name), Some(email)) => format!("{name} <{email}>"),
            (_, Some(email)) => email.to_string(),
            _ => String::new(),
        })
        .unwrap_or_default()
}

fn unread(message: &Value) -> bool {
    message["keywords"]["$seen"].as_bool() != Some(true)
}

fn summary(m: &Value) -> Value {
    json!({
        "id": m["id"], "from": from_line(&m["from"]), "subject": m["subject"].as_str().unwrap_or("(no subject)"), "preview": m["preview"].as_str().unwrap_or_default(),
        "received_at": m["receivedAt"], "unread": unread(m), "attachments": m["hasAttachment"].as_bool().unwrap_or(false),
    })
}

/// Text of a message: the plain parts, or the HTML parts with tags and scripts removed.
pub fn body_text(message: &Value) -> String {
    let values = &message["bodyValues"];
    let parts: Vec<String> = message["textBody"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let text = values[p["partId"].as_str()?]["value"].as_str()?;
            Some(if p["type"].as_str() == Some("text/html") { strip_html(text) } else { text.to_string() })
        })
        .collect();
    parts.join("\n\n")
}

pub fn strip_html(html: &str) -> String {
    let mut out = String::new();
    let (mut in_tag, mut skip_until): (bool, Option<&str>) = (false, None);
    let lower = html.to_lowercase();
    let mut i = 0;
    while i < html.len() {
        if let Some(end) = skip_until {
            match lower[i..].find(end) {
                Some(at) => {
                    i += at + end.len();
                    skip_until = None;
                    continue;
                }
                None => break,
            }
        }
        let c = html[i..].chars().next().unwrap_or(' ');
        if in_tag {
            if c == '>' {
                in_tag = false;
            }
        } else if c == '<' {
            if lower[i..].starts_with("<script") {
                skip_until = Some("</script>");
                continue;
            }
            if lower[i..].starts_with("<style") {
                skip_until = Some("</style>");
                continue;
            }
            if ["<br", "</p", "</div", "</li", "</tr"].iter().any(|t| lower[i..].starts_with(t)) {
                out.push('\n');
            }
            in_tag = true;
        } else {
            out.push(c);
        }
        i += c.len_utf8();
    }
    out.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'")
}

fn addresses(params: &Value, name: &str) -> Result<Vec<Value>, SendError> {
    let list = params[name].as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for a in list {
        let a = a.as_str().unwrap_or_default().trim().to_string();
        if a.is_empty() || a.len() > 320 || !a.contains('@') || a.contains(['\r', '\n', '<', '>', ',', ' ']) {
            return Err(SendError::permanent(format!("`{name}` holds an address that is not valid: {a}")));
        }
        out.push(json!({ "email": a }));
    }
    Ok(out)
}

fn id_of(params: &Value, name: &str) -> Result<String, SendError> {
    let id = params[name].as_str().unwrap_or_default();
    if id.is_empty() || id.len() > 255 || !id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) {
        return Err(SendError::permanent(format!("`{name}` is required: a message or mailbox id")));
    }
    Ok(id.to_string())
}

fn message_ids(params: &Value, name: &str) -> Vec<String> {
    let text = |v: &Value| v.as_str().map(|s| s.trim().trim_matches(['<', '>']).to_string()).filter(|s| !s.is_empty() && !s.contains(['\r', '\n', ' ']));
    match &params[name] {
        Value::String(s) => s.split_whitespace().filter_map(|p| text(&json!(p))).collect(),
        Value::Array(a) => a.iter().filter_map(text).collect(),
        _ => Vec::new(),
    }
}

#[async_trait]
impl ActionBridge for Jmap {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, SendError> {
        if !ACTIONS.iter().any(|a| a.name == action) {
            return Err(SendError::permanent(format!("jmap has no action `{action}`; it offers: {}", ACTIONS.iter().map(|a| a.name).collect::<Vec<_>>().join(", "))));
        }
        let login = Login {
            username: params["_credential"]["username"].as_str().unwrap_or_default().to_string(),
            secret: params["_credential"]["secret"].as_str().unwrap_or_default().to_string(),
        };
        if login.username.is_empty() {
            return Err(SendError::permanent("you have not connected your mailbox yet"));
        }
        let http = client()?;
        let session = self.session(&http, &login).await?;
        let acc = session.account.clone();
        match action {
            "check" => Ok(json!({ "name": login.username, "account": acc })),
            "mailboxes" => {
                let a = self.invoke(&http, &login, &session, vec![json!(["Mailbox/get", { "accountId": acc, "properties": ["id", "name", "role", "totalEmails", "unreadEmails", "sortOrder"] }, "m"])]).await?;
                let mut rows: Vec<Value> = answer(&a, "m")?["list"].as_array().cloned().unwrap_or_default();
                rows.sort_by_key(|r| (r["sortOrder"].as_i64().unwrap_or(1000), r["name"].as_str().unwrap_or_default().to_lowercase()));
                Ok(Value::Array(rows.iter().map(|r| json!({ "id": r["id"], "name": r["name"], "role": r["role"], "total": r["totalEmails"], "unread": r["unreadEmails"] })).collect()))
            }
            "list" => {
                let limit = params["limit"].as_u64().unwrap_or(25);
                if !(1..=50).contains(&limit) {
                    return Err(SendError::permanent("`limit` is 1 to 50"));
                }
                let mut filter = serde_json::Map::new();
                if let Some(text) = params["search"].as_str().map(str::trim).filter(|t| !t.is_empty()) {
                    filter.insert("text".into(), json!(text.chars().take(200).collect::<String>()));
                }
                match params["mailbox"].as_str().filter(|m| !m.is_empty()) {
                    Some(_) => {
                        filter.insert("inMailbox".into(), json!(id_of(params, "mailbox")?));
                    }
                    None if filter.is_empty() => {
                        let a = self.invoke(&http, &login, &session, vec![json!(["Mailbox/get", { "accountId": acc, "properties": ["id", "role"] }, "m"])]).await?;
                        let inbox = answer(&a, "m")?["list"].as_array().and_then(|l| l.iter().find(|m| m["role"].as_str() == Some("inbox"))).and_then(|m| m["id"].as_str().map(str::to_string));
                        if let Some(inbox) = inbox {
                            filter.insert("inMailbox".into(), json!(inbox));
                        }
                    }
                    None => {}
                }
                let a = self
                    .invoke(
                        &http,
                        &login,
                        &session,
                        vec![
                            json!(["Email/query", { "accountId": acc, "filter": filter, "sort": [{ "property": "receivedAt", "isAscending": false }], "position": params["position"].as_u64().unwrap_or(0), "limit": limit, "calculateTotal": true }, "q"]),
                            json!(["Email/get", { "accountId": acc, "#ids": { "resultOf": "q", "name": "Email/query", "path": "/ids" }, "properties": ["id", "from", "subject", "receivedAt", "preview", "keywords", "hasAttachment"] }, "g"]),
                        ],
                    )
                    .await?;
                let total = answer(&a, "q")?["total"].as_u64().unwrap_or(0);
                let messages: Vec<Value> = answer(&a, "g")?["list"].as_array().into_iter().flatten().map(summary).collect();
                Ok(json!({ "messages": messages, "total": total }))
            }
            "get" => {
                let id = id_of(params, "id")?;
                let a = self
                    .invoke(
                        &http,
                        &login,
                        &session,
                        vec![json!(["Email/get", { "accountId": acc, "ids": [id], "properties": ["id", "messageId", "references", "from", "to", "cc", "subject", "receivedAt", "keywords", "attachments", "textBody", "bodyValues"], "fetchTextBodyValues": true, "maxBodyValueBytes": 200_000 }, "g"])],
                    )
                    .await?;
                let m = answer(&a, "g")?["list"].as_array().and_then(|l| l.first()).cloned().ok_or_else(|| SendError::permanent("there is no such message"))?;
                Ok(json!({
                    "id": m["id"], "from": person(&m["from"]), "to": person(&m["to"]), "cc": person(&m["cc"]), "subject": m["subject"].as_str().unwrap_or("(no subject)"),
                    "received_at": m["receivedAt"], "text": body_text(&m), "message_id": m["messageId"][0], "references": m["references"], "unread": unread(&m),
                    "attachments": m["attachments"].as_array().into_iter().flatten().map(|p| json!({ "name": p["name"], "type": p["type"], "size": p["size"] })).collect::<Vec<_>>(),
                }))
            }
            "attachment" => {
                let id = id_of(params, "id")?;
                let index = params["index"].as_u64().unwrap_or(u64::MAX) as usize;
                let a = self.invoke(&http, &login, &session, vec![json!(["Email/get", { "accountId": acc, "ids": [id], "properties": ["attachments"] }, "g"])]).await?;
                let m = answer(&a, "g")?["list"].as_array().and_then(|l| l.first()).cloned().ok_or_else(|| SendError::permanent("there is no such message"))?;
                let part = m["attachments"].as_array().and_then(|l| l.get(index)).cloned().ok_or_else(|| SendError::permanent("the message has no such attachment"))?;
                if part["size"].as_u64().unwrap_or(0) as usize > MAX_ATTACHMENT_BYTES {
                    return Err(SendError::permanent("that attachment is larger than 10 MB"));
                }
                let (blob, name, kind) = (part["blobId"].as_str().unwrap_or_default(), part["name"].as_str().unwrap_or("attachment"), part["type"].as_str().unwrap_or("application/octet-stream"));
                if session.download_url.is_empty() || blob.is_empty() {
                    return Err(SendError::permanent("the mail server offers no downloads"));
                }
                let url = fill(&session.download_url, &[("accountId", &acc), ("blobId", blob), ("name", name), ("type", kind)]);
                let response = http.get(url).basic_auth(&login.username, Some(&login.secret)).send().await.map_err(network)?;
                if !response.status().is_success() {
                    return Err(refused(response.status().as_u16()));
                }
                let bytes = response.bytes().await.map_err(network)?;
                if bytes.len() > MAX_ATTACHMENT_BYTES {
                    return Err(SendError::permanent("that attachment is larger than 10 MB"));
                }
                Ok(json!({ "name": name, "type": kind, "size": bytes.len(), "base64": STANDARD.encode(&bytes) }))
            }
            "mark" => {
                let id = id_of(params, "id")?;
                let seen = if params["read"].as_bool().unwrap_or(true) { json!(true) } else { Value::Null };
                let a = self.invoke(&http, &login, &session, vec![json!(["Email/set", { "accountId": acc, "update": { id.clone(): { "keywords/$seen": seen } } }, "s"])]).await?;
                if answer(&a, "s")?["notUpdated"].get(&id).is_some() {
                    return Err(SendError::permanent("the mail server would not change that message"));
                }
                Ok(json!({ "ok": true }))
            }
            "trash" => {
                let id = id_of(params, "id")?;
                let a = self.invoke(&http, &login, &session, vec![json!(["Mailbox/get", { "accountId": acc, "properties": ["id", "role"] }, "m"])]).await?;
                let trash = answer(&a, "m")?["list"].as_array().and_then(|l| l.iter().find(|m| m["role"].as_str() == Some("trash"))).and_then(|m| m["id"].as_str().map(str::to_string)).ok_or_else(|| SendError::permanent("this mailbox has no trash folder"))?;
                let a = self.invoke(&http, &login, &session, vec![json!(["Email/set", { "accountId": acc, "update": { id.clone(): { "mailboxIds": { trash: true } } } }, "s"])]).await?;
                if answer(&a, "s")?["notUpdated"].get(&id).is_some() {
                    return Err(SendError::permanent("the mail server would not move that message"));
                }
                Ok(json!({ "ok": true }))
            }
            "send" => {
                let to = addresses(params, "to")?;
                if to.is_empty() || to.len() + addresses(params, "cc")?.len() > 50 {
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
                let prep = self
                    .invoke(
                        &http,
                        &login,
                        &session,
                        vec![json!(["Identity/get", { "accountId": acc }, "i"]), json!(["Mailbox/get", { "accountId": acc, "properties": ["id", "role"] }, "m"])],
                    )
                    .await?;
                let identity = answer(&prep, "i")?["list"].as_array().and_then(|l| l.first()).cloned().ok_or_else(|| SendError::permanent("this login has no address to send from"))?;
                let role = |r: &str| answer(&prep, "m").ok().and_then(|m| m["list"].as_array().and_then(|l| l.iter().find(|x| x["role"].as_str() == Some(r))).and_then(|x| x["id"].as_str().map(str::to_string)));
                let (drafts, sent) = (role("drafts").ok_or_else(|| SendError::permanent("this mailbox has no drafts folder"))?, role("sent"));
                let mut parts = Vec::new();
                let mut total = 0usize;
                for item in params["attachments"].as_array().into_iter().flatten().take(20) {
                    let bytes = STANDARD.decode(item["base64"].as_str().unwrap_or_default()).map_err(|_| SendError::permanent("an attachment is not valid base64"))?;
                    total += bytes.len();
                    if bytes.is_empty() || total > MAX_ATTACHMENT_BYTES {
                        return Err(SendError::permanent("attachments are at most 10 MB in all, none empty"));
                    }
                    let kind = item["type"].as_str().filter(|t| !t.is_empty() && !t.contains(['\r', '\n'])).unwrap_or("application/octet-stream").to_string();
                    let name = item["name"].as_str().unwrap_or("attachment").replace(['\r', '\n', '"'], "_");
                    if session.upload_url.is_empty() {
                        return Err(SendError::permanent("the mail server accepts no uploads"));
                    }
                    let response = http.post(fill(&session.upload_url, &[("accountId", &acc)])).basic_auth(&login.username, Some(&login.secret)).header("Content-Type", kind.clone()).body(bytes).send().await.map_err(network)?;
                    if !response.status().is_success() {
                        return Err(refused(response.status().as_u16()));
                    }
                    let up: Value = response.json().await.map_err(|_| SendError::transient("the mail server sent something that is not JSON"))?;
                    parts.push(json!({ "blobId": up["blobId"], "type": kind, "name": name, "disposition": "attachment" }));
                }
                let mut email = json!({
                    "mailboxIds": { drafts.clone(): true }, "keywords": { "$draft": true },
                    "from": [{ "name": identity["name"], "email": identity["email"] }], "to": to, "cc": addresses(params, "cc")?, "subject": subject,
                    "textBody": [{ "partId": "t", "type": "text/plain" }], "bodyValues": { "t": { "value": text } },
                });
                if !parts.is_empty() {
                    email["attachments"] = json!(parts);
                }
                let (reply, refs) = (message_ids(params, "in_reply_to"), message_ids(params, "references"));
                if !reply.is_empty() {
                    email["header:In-Reply-To:asMessageIds"] = json!(reply);
                }
                if !refs.is_empty() {
                    email["header:References:asMessageIds"] = json!(refs);
                }
                let mut after = json!({ "keywords/$draft": Value::Null });
                after[format!("mailboxIds/{drafts}")] = Value::Null;
                if let Some(sent) = &sent {
                    after[format!("mailboxIds/{sent}")] = json!(true);
                }
                let a = self
                    .invoke(
                        &http,
                        &login,
                        &session,
                        vec![
                            json!(["Email/set", { "accountId": acc, "create": { "draft": email } }, "e"]),
                            json!(["EmailSubmission/set", { "accountId": acc, "create": { "s": { "identityId": identity["id"], "emailId": "#draft" } }, "onSuccessUpdateEmail": { "#s": after } }, "s"]),
                        ],
                    )
                    .await?;
                if let Some(why) = answer(&a, "e")?["notCreated"]["draft"].as_object() {
                    return Err(SendError::permanent(format!("the mail server refused the message: {}", why.get("description").and_then(Value::as_str).unwrap_or("not accepted"))));
                }
                if let Some(why) = answer(&a, "s")?["notCreated"]["s"].as_object() {
                    return Err(SendError::permanent(format!("the mail server would not send it: {}", why.get("description").and_then(Value::as_str).unwrap_or("not accepted"))));
                }
                Ok(json!({ "id": answer(&a, "e")?["created"]["draft"]["id"] }))
            }
            _ => unreachable!("checked above"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_only_mail_becomes_text_without_scripts() {
        let m = json!({ "textBody": [{ "partId": "1", "type": "text/html" }], "bodyValues": { "1": { "value": "<style>p{}</style><p>Hello &amp; welcome</p><script>alert(1)</script><p>Bye<br>now</p>" } } });
        assert_eq!(body_text(&m).trim(), "Hello & welcome\nBye\nnow");
        let plain = json!({ "textBody": [{ "partId": "1", "type": "text/plain" }], "bodyValues": { "1": { "value": "a <b> c" } } });
        assert_eq!(body_text(&plain), "a <b> c");
    }

    #[test]
    fn summaries_say_who_and_whether_unread() {
        let m = json!({ "id": "e1", "from": [{ "name": "Ann", "email": "ann@x.test" }], "subject": "Hi", "keywords": { "$seen": true }, "hasAttachment": true });
        let s = summary(&m);
        assert_eq!((s["from"].as_str(), s["unread"].as_bool(), s["attachments"].as_bool()), (Some("Ann <ann@x.test>"), Some(false), Some(true)));
        assert_eq!(summary(&json!({ "id": "e2", "from": [{ "email": "b@x.test" }] }))["unread"], true);
    }

    #[test]
    fn addresses_and_ids_are_checked() {
        assert!(addresses(&json!({ "to": ["a@x.test", "b c@x.test"] }), "to").is_err());
        assert!(addresses(&json!({ "to": ["a@x.test\r\nBcc: z@x.test"] }), "to").is_err(), "header injection");
        assert_eq!(addresses(&json!({ "to": ["a@x.test"] }), "to").map(|a| a.len()), Ok(1));
        assert!(id_of(&json!({ "id": "../x" }), "id").is_err());
        assert_eq!(message_ids(&json!({ "references": "<a@x> <b@x>" }), "references"), ["a@x", "b@x"]);
    }

    #[tokio::test]
    async fn a_call_without_a_login_or_server_is_refused() {
        let b = Jmap::new(&Values::new()).unwrap_or_else(|e| panic!("{e}"));
        assert!(b.call("list", &json!({})).await.err().is_some_and(|e| e.permanent && e.message.contains("connected")));
        let login = json!({ "_credential": { "username": "a", "secret": "b" } });
        assert!(b.call("list", &login).await.err().is_some_and(|e| e.message.contains("session_url")));
        assert!(b.call("fly", &login).await.err().is_some_and(|e| e.permanent));
    }
}
