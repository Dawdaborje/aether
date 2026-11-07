//! Tying mail to records: a short token in an address (`support+tk_k3j9x0a2bq@mail.acme.test`) or in a
//! `Message-ID` (`<tk_k3j9x0a2bq.7@mail.acme.test>`) names one record, so a reply finds it again.

use rand::RngExt;
use serde_json::Value;

use crate::databases::Db;

const PREFIX: &str = "tk_";
const TOKEN_LEN: usize = 10;

/// The token in a plus-tag or a Message-ID, if there is one.
pub fn token_in(text: &str) -> Option<String> {
    let at = text.find(PREFIX)?;
    let token: String = text[at + PREFIX.len()..]
        .chars()
        .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .collect();
    (token.len() == TOKEN_LEN).then_some(token)
}

/// `(alias, tag)` of a local part: `support+tk_x` is `("support", Some("tk_x"))`.
pub fn split_local(address: &str) -> (&str, Option<&str>) {
    let local = address.split('@').next().unwrap_or_default();
    match local.split_once('+') {
        Some((alias, tag)) => (alias, Some(tag)),
        None => (local, None),
    }
}

pub fn domain_of(address: &str) -> &str {
    address.rsplit_once('@').map_or("", |(_, domain)| domain)
}

/// The address to put in `Reply-To` so that an answer finds `token`'s record.
pub fn reply_address(alias: &str, token: &str, domain: &str) -> String {
    format!("{alias}+{PREFIX}{token}@{domain}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    pub plugin: String,
    pub model_id: String,
    pub record_key: String,
}

fn new_token() -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..TOKEN_LEN).map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char).collect()
}

/// The record's token, made on first use.
pub async fn ensure(db: &Db, plugin: &str, model_id: &str, record_key: &str) -> Result<String, surrealdb::Error> {
    for _ in 0..4 {
        let mut found = db
            .query("SELECT VALUE token FROM mail_threads WHERE plugin = $p AND model_id = $m AND record_key = $k LIMIT 1;")
            .bind(("p", plugin.to_string()))
            .bind(("m", model_id.to_string()))
            .bind(("k", record_key.to_string()))
            .await?
            .check()?;
        let existing: Vec<String> = found.take(0)?;
        if let Some(token) = existing.into_iter().next() {
            return Ok(token);
        }
        let token = new_token();
        // A token or record that raced us is a unique-index refusal: look again.
        let made = db
            .query("CREATE mail_threads SET token = $t, plugin = $p, model_id = $m, record_key = $k;")
            .bind(("t", token.clone()))
            .bind(("p", plugin.to_string()))
            .bind(("m", model_id.to_string()))
            .bind(("k", record_key.to_string()))
            .await
            .and_then(|r| r.check());
        if made.is_ok() {
            return Ok(token);
        }
    }
    Err(surrealdb::Error::internal("could not make a mail token".to_string()))
}

pub async fn find(db: &Db, token: &str) -> Result<Option<Thread>, surrealdb::Error> {
    let mut response = db
        .query("SELECT plugin, model_id, record_key FROM mail_threads WHERE token = $t LIMIT 1;")
        .bind(("t", token.to_string()))
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    Ok(rows.into_iter().next().and_then(|row| {
        Some(Thread {
            plugin: row.get("plugin")?.as_str()?.to_string(),
            model_id: row.get("model_id")?.as_str()?.to_string(),
            record_key: row.get("record_key")?.as_str()?.to_string(),
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_found_in_an_address_or_a_message_id() {
        assert_eq!(token_in("tk_k3j9x0a2bq").as_deref(), Some("k3j9x0a2bq"));
        assert_eq!(token_in("tk_k3j9x0a2bq.7@mail.acme.test").as_deref(), Some("k3j9x0a2bq"));
        assert_eq!(token_in("tk_short"), None, "wrong length");
        assert_eq!(token_in("hello"), None);
    }

    #[test]
    fn an_address_splits_into_alias_and_tag() {
        assert_eq!(split_local("support+tk_x@d.test"), ("support", Some("tk_x")));
        assert_eq!(split_local("support@d.test"), ("support", None));
        assert_eq!(domain_of("a@mail.acme.test"), "mail.acme.test");
        assert_eq!(reply_address("support", "k3j9x0a2bq", "mail.acme.test"), "support+tk_k3j9x0a2bq@mail.acme.test");
        assert_eq!(token_in(split_local(&reply_address("support", "k3j9x0a2bq", "d.test")).1.unwrap()).as_deref(), Some("k3j9x0a2bq"));
    }

    #[test]
    fn tokens_are_the_right_length_and_alphabet() {
        let t = new_token();
        assert_eq!(t.len(), TOKEN_LEN);
        assert!(t.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }
}
