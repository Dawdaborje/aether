//! The audit chain: making the audit trail tamper-evident (see `docs/architecture/audit_chain.md`).
//!
//! The trail is written in the same transaction as the work, so a write without an audit row cannot
//! happen. This makes it **detectable** when someone with database access later changes, removes or
//! reorders audit rows. It does not stop them; it stops them doing it silently.
//!
//! Each audit table is a chain. A background sealer ([`seal`]) numbers the rows that are not yet
//! sealed (`chain_seq` 1, 2, 3 ... per table) and links each to the one before: `row_hash` is the
//! SHA-256 of the row's canonical form, which includes the previous row's hash. Sealing is off the
//! request path on purpose: reads are audited too, and a counter in every request would make every
//! read in an organization wait for one row. The price is a short window (seconds) in which a row is
//! written but not yet sealed; [`verify`] reports how many rows are in it.
//!
//! Every so often a signed [`checkpoint`](Checkpoint) says "up to `to_seq` the chain's hash was
//! `head_hash`". Signatures are Ed25519 by the installation's [`AuditKey`]; the public keys are kept
//! in `audit_keys`, so old checkpoints keep verifying after the key is replaced. A checkpoint can be
//! exported as an [`Anchor`] and kept somewhere the server's administrator cannot rewrite.
//!
//! # Canonical form
//!
//! Specified here so a verifier in any language can be written. The text hashed for a row is the
//! parts below joined with `|`:
//!
//! 1. the format name `aether-audit-v1`
//! 2. the chain name (`data_access`, `plugin_calls` or `page_visits`)
//! 3. the row's `chain_seq` in decimal
//! 4. the previous row's `row_hash` in lower-case hex (empty for the first row, or the chain's
//!    `base_hash` after a purge)
//! 5. `date_created` as whole nanoseconds since the epoch, in decimal
//! 6. the chain's fields, in the order of [`Chain::fields`], each rendered as: `-` when missing or
//!    null; `=` and the value for text (with `\`, `|`, `,` and line feed written `\\`, `\|`, `\,`,
//!    `\n`), a number or a boolean; for a list, `[` and its items rendered the same way joined with
//!    `,` and `]`.
//!
//! The hash is the lower-case hex SHA-256 of that text in UTF-8.
//!
//! A checkpoint is signed over `aether-audit-checkpoint-v1|<org>|<chain>|<to_seq>|<head_hash>|<at_ns>`.

use std::path::Path;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use surrealdb::{Surreal, engine::remote::ws::Client};
use thiserror::Error;

/// The name of the canonical form, the first part of every hashed row.
pub const FORMAT: &str = "aether-audit-v1";
const CHECKPOINT_FORMAT: &str = "aether-audit-checkpoint-v1";
/// Rows read from the database at a time while verifying.
const PAGE: u32 = 1000;
/// Failures a report lists; the rest are only counted.
const MAX_FAILURES: usize = 20;
/// Checkpoints whose row is compared against the chain on a verify (all are checked for signature).
const CHECKPOINTS_MATCHED: usize = 50;

#[derive(Debug, Error)]
pub enum ChainError {
    #[error("audit chain database error: {0}")]
    Database(#[from] surrealdb::Error),
    #[error("audit chain: {0}")]
    Format(String),
    #[error("audit key: {0}")]
    Key(String),
}

/// One of the three audit tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Chain {
    DataAccess,
    PluginCalls,
    PageVisits,
}

impl Chain {
    pub const ALL: [Chain; 3] = [Chain::DataAccess, Chain::PluginCalls, Chain::PageVisits];

    /// The table, which is also the chain's name.
    pub fn name(self) -> &'static str {
        match self {
            Self::DataAccess => "data_access",
            Self::PluginCalls => "plugin_calls",
            Self::PageVisits => "page_visits",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|chain| chain.name() == name)
    }

    /// The columns of a row that are hashed, in order (after the number, the previous hash and
    /// the time).
    pub fn fields(self) -> &'static [&'static str] {
        match self {
            Self::DataAccess => &[
                "request_id",
                "actor_type",
                "actor_id",
                "plugin",
                "function_name",
                "model",
                "table_name",
                "operation",
                "record_ids",
                "record_count",
                "statement",
                "ip",
            ],
            Self::PluginCalls => &[
                "request_id",
                "actor_type",
                "actor_id",
                "plugin",
                "function_name",
                "status",
                "ip",
                "user_agent",
            ],
            Self::PageVisits => &[
                "request_id",
                "actor_type",
                "actor_id",
                "plugin",
                "route",
                "path",
                "method",
                "status",
                "ip",
                "user_agent",
            ],
        }
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '|' => out.push_str("\\|"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out
}

fn render(value: &Value) -> String {
    match value {
        Value::Null => "-".into(),
        Value::String(text) => format!("={}", escape(text)),
        Value::Number(number) => format!("={number}"),
        Value::Bool(flag) => format!("={flag}"),
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(render).collect();
            format!("[{}]", items.join(","))
        }
        // No audit column holds an object; written down anyway so the form has no gaps.
        Value::Object(_) => format!("={}", escape(&value.to_string())),
    }
}

/// A time as the database returns it (RFC 3339) in whole nanoseconds since the epoch.
fn nanos(value: &Value) -> Result<i64, ChainError> {
    let text = value
        .as_str()
        .ok_or_else(|| ChainError::Format("a row has no `date_created`".into()))?;
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .and_then(|time| time.timestamp_nanos_opt())
        .ok_or_else(|| ChainError::Format(format!("`date_created` is not a time: {text}")))
}

/// The text that is hashed for `row` as number `seq` of `chain`, after a row hashing to `prev_hash`.
pub fn canonical(chain: Chain, seq: u64, prev_hash: &str, row: &Value) -> Result<String, ChainError> {
    let mut parts = vec![
        FORMAT.to_string(),
        chain.name().to_string(),
        seq.to_string(),
        prev_hash.to_string(),
        nanos(row.get("date_created").unwrap_or(&Value::Null))?.to_string(),
    ];
    for field in chain.fields() {
        parts.push(render(row.get(*field).unwrap_or(&Value::Null)));
    }
    Ok(parts.join("|"))
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 || !text.is_ascii() {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

/// The hash of a row: lower-case hex SHA-256 of its [`canonical`] text.
pub fn row_hash(chain: Chain, seq: u64, prev_hash: &str, row: &Value) -> Result<String, ChainError> {
    Ok(to_hex(&Sha256::digest(canonical(chain, seq, prev_hash, row)?.as_bytes())))
}

fn checkpoint_message(org: &str, chain: &str, to_seq: u64, head_hash: &str, at_ns: i64) -> String {
    format!("{CHECKPOINT_FORMAT}|{org}|{chain}|{to_seq}|{head_hash}|{at_ns}")
}

/// The key that signs checkpoints. One per installation, kept in `<app_dir>/conf/audit.key`.
pub struct AuditKey {
    signing: SigningKey,
    key_id: String,
}

impl AuditKey {
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let signing = SigningKey::from_bytes(&seed);
        let key_id = to_hex(&Sha256::digest(signing.verifying_key().as_bytes()))[..16].to_string();
        Self { signing, key_id }
    }

    /// Read the key file, or make one (mode 0600) the first time.
    pub fn load_or_create(path: &Path) -> Result<Self, ChainError> {
        let io = |source: std::io::Error| ChainError::Key(format!("{}: {source}", path.display()));
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let seed = from_hex(text.trim())
                    .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
                    .ok_or_else(|| ChainError::Key(format!("{} is not a 32-byte hex key", path.display())))?;
                Ok(Self::from_seed(seed))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut seed = [0u8; 32];
                rand::RngExt::fill(&mut rand::rng(), &mut seed);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(io)?;
                }
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                use std::io::Write;
                options
                    .open(path)
                    .and_then(|mut file| file.write_all(to_hex(&seed).as_bytes()))
                    .map_err(io)?;
                log::info!(
                    "Created the audit key file {}; keep it with your backups",
                    path.display()
                );
                Ok(Self::from_seed(seed))
            }
            Err(error) => Err(io(error)),
        }
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// The public key, as hex.
    pub fn public_hex(&self) -> String {
        to_hex(self.signing.verifying_key().as_bytes())
    }

    fn sign(&self, message: &str) -> String {
        to_hex(&self.signing.sign(message.as_bytes()).to_bytes())
    }
}

/// Whether `signature_hex` is a valid signature of `message` by the public key `public_hex`.
pub fn signature_ok(public_hex: &str, message: &str, signature_hex: &str) -> bool {
    let key = from_hex(public_hex)
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .and_then(|bytes| VerifyingKey::from_bytes(&bytes).ok());
    let signature = from_hex(signature_hex)
        .and_then(|bytes| <[u8; 64]>::try_from(bytes).ok())
        .map(|bytes| Signature::from_bytes(&bytes));
    match (key, signature) {
        (Some(key), Some(signature)) => key.verify(message.as_bytes(), &signature).is_ok(),
        _ => false,
    }
}

/// Where a chain stands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Head {
    pub seq: u64,
    pub hash: String,
    /// Rows up to this number were purged; the first kept row links to `base_hash`.
    pub base_seq: u64,
    pub base_hash: String,
    pub checkpoint_seq: u64,
    pub checkpoint_ns: i64,
}

impl Head {
    fn from_row(row: &Value) -> Self {
        let number = |key: &str| row.get(key).and_then(Value::as_u64).unwrap_or(0);
        let text = |key: &str| row.get(key).and_then(Value::as_str).unwrap_or("").to_string();
        Self {
            seq: number("seq"),
            hash: text("hash"),
            base_seq: number("base_seq"),
            base_hash: text("base_hash"),
            checkpoint_seq: number("checkpoint_seq"),
            checkpoint_ns: row.get("checkpoint_ns").and_then(Value::as_i64).unwrap_or(0),
        }
    }
}

pub async fn head(db: &Surreal<Client>, chain: Chain) -> Result<Head, ChainError> {
    let mut response = db
        .query("SELECT * FROM audit_chain_head WHERE chain = $chain LIMIT 1;")
        .bind(("chain", chain.name().to_string()))
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    Ok(rows.first().map(Head::from_row).unwrap_or_default())
}

fn now_ns() -> i64 {
    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
}

/// How sealing behaves.
#[derive(Debug, Clone)]
pub struct SealOptions {
    /// Rows sealed in one transaction.
    pub batch: u32,
    /// A checkpoint is made when this many rows were sealed since the last one ...
    pub checkpoint_rows: u64,
    /// ... or when the last one is this old.
    pub checkpoint_secs: u64,
}

impl Default for SealOptions {
    fn default() -> Self {
        Self {
            batch: 500,
            checkpoint_rows: 10_000,
            checkpoint_secs: 3600,
        }
    }
}

/// What one sealing pass did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Sealed {
    pub sealed: u64,
    /// The `to_seq` of the checkpoint made, if one was.
    pub checkpoint: Option<u64>,
}

/// Seal the rows of `chain` that are not sealed yet (at most one batch), then make a checkpoint if
/// one is due. Safe to run from several servers at once: a pass that finds the chain has moved on
/// does nothing and the other one's work stands.
pub async fn seal(
    db: &Surreal<Client>,
    org: &str,
    chain: Chain,
    key: &AuditKey,
    options: &SealOptions,
) -> Result<Sealed, ChainError> {
    let mut head = head(db, chain).await?;
    let mut response = db
        .query(format!(
            "SELECT * FROM {} WHERE chain_seq = NONE ORDER BY date_created, id LIMIT $limit;",
            chain.name()
        ))
        .bind(("limit", options.batch))
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;

    let mut sealed = 0;
    if !rows.is_empty() {
        let expected = head.seq;
        // Hashing a batch is CPU work (much of a second in a debug build): it runs on a blocking
        // thread so the runtime's workers keep answering requests meanwhile.
        let (start_seq, start_prev) = (head.seq, head.hash.clone());
        let (updates, seq, prev) = tokio::task::spawn_blocking(move || {
            let (mut seq, mut prev) = (start_seq, start_prev);
            let mut updates = Vec::with_capacity(rows.len());
            for row in &rows {
                let Some(id) = row.get("id").and_then(Value::as_str) else {
                    continue;
                };
                seq += 1;
                let hash = row_hash(chain, seq, &prev, row)?;
                updates.push(json!({ "id": id, "seq": seq, "prev": prev, "hash": hash }));
                prev = hash;
            }
            Ok::<_, ChainError>((updates, seq, prev))
        })
        .await
        .map_err(|error| ChainError::Format(format!("sealing stopped: {error}")))??;
        if updates.is_empty() {
            return Ok(Sealed::default());
        }
        let count = updates.len() as u64;
        let result = db
            .query(
                "BEGIN TRANSACTION; \
                 LET $now_head = (SELECT VALUE seq FROM audit_chain_head WHERE chain = $chain)[0] ?? 0; \
                 IF $now_head != $expected { THROW 'aether: the chain moved'; }; \
                 FOR $u IN $updates { \
                     UPDATE type::record($u.id) SET chain_seq = $u.seq, prev_hash = $u.prev, row_hash = $u.hash; \
                 }; \
                 UPSERT audit_chain_head SET chain = $chain WHERE chain = $chain; \
                 UPDATE audit_chain_head SET seq = $seq, hash = $hash, \
                     started_ns = IF started_ns = 0 THEN $now ELSE started_ns END WHERE chain = $chain; \
                 COMMIT TRANSACTION;",
            )
            .bind(("chain", chain.name().to_string()))
            .bind(("expected", expected))
            .bind(("updates", updates))
            .bind(("seq", seq))
            .bind(("hash", prev.clone()))
            .bind(("now", now_ns()))
            .await;
        match result.and_then(|response| response.check()) {
            Ok(_) => {}
            // Another server sealed first: its work stands, this pass has nothing left to do.
            Err(error) if error.to_string().contains("the chain moved") => return Ok(Sealed::default()),
            Err(error) => return Err(error.into()),
        }
        sealed = count;
        head.seq = seq;
        head.hash = prev;
    }

    let due = head.seq > head.checkpoint_seq
        && (head.seq - head.checkpoint_seq >= options.checkpoint_rows
            || now_ns() - head.checkpoint_ns >= (options.checkpoint_secs as i64).saturating_mul(1_000_000_000));
    let checkpoint = if due {
        checkpoint(db, org, chain, &head, key).await?.then_some(head.seq)
    } else {
        None
    };
    Ok(Sealed { sealed, checkpoint })
}

/// Sign and store a checkpoint of the chain as `head` stands. `false` when one for that point
/// already exists (another server made it).
pub async fn checkpoint(
    db: &Surreal<Client>,
    org: &str,
    chain: Chain,
    head: &Head,
    key: &AuditKey,
) -> Result<bool, ChainError> {
    let at_ns = now_ns();
    let signature = key.sign(&checkpoint_message(org, chain.name(), head.seq, &head.hash, at_ns));
    let result = db
        .query(
            "BEGIN TRANSACTION; \
             UPSERT audit_keys SET key_id = $key_id, public_key = $public WHERE key_id = $key_id; \
             CREATE audit_checkpoints SET chain = $chain, org = $org, from_seq = $from, to_seq = $to, \
                 head_hash = $hash, at_ns = $at, key_id = $key_id, signature = $signature; \
             UPDATE audit_chain_head SET checkpoint_seq = $to, checkpoint_ns = $at WHERE chain = $chain; \
             COMMIT TRANSACTION;",
        )
        .bind(("key_id", key.key_id().to_string()))
        .bind(("public", key.public_hex()))
        .bind(("chain", chain.name().to_string()))
        .bind(("org", org.to_string()))
        .bind(("from", head.checkpoint_seq + 1))
        .bind(("to", head.seq))
        .bind(("hash", head.hash.clone()))
        .bind(("at", at_ns))
        .bind(("signature", signature))
        .await;
    match result.and_then(|response| response.check()) {
        Ok(_) => Ok(true),
        Err(error) if error.to_string().contains("already contains") => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// A checkpoint kept outside the database: enough to prove, later, that the chain up to `to_seq`
/// was as it is here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    pub format: String,
    pub org: String,
    pub chain: String,
    pub to_seq: u64,
    pub head_hash: String,
    pub at_ns: i64,
    pub key_id: String,
    pub signature: String,
}

impl Anchor {
    fn from_row(row: &Value) -> Option<Self> {
        Some(Self {
            format: CHECKPOINT_FORMAT.to_string(),
            org: row.get("org")?.as_str()?.to_string(),
            chain: row.get("chain")?.as_str()?.to_string(),
            to_seq: row.get("to_seq")?.as_u64()?,
            head_hash: row.get("head_hash")?.as_str()?.to_string(),
            at_ns: row.get("at_ns")?.as_i64()?,
            key_id: row.get("key_id")?.as_str()?.to_string(),
            signature: row.get("signature")?.as_str()?.to_string(),
        })
    }

    fn message(&self) -> String {
        checkpoint_message(&self.org, &self.chain, self.to_seq, &self.head_hash, self.at_ns)
    }
}

/// The newest checkpoint of a chain, as an anchor to keep. `None` when there is none yet.
pub async fn export_anchor(db: &Surreal<Client>, chain: Chain) -> Result<Option<Anchor>, ChainError> {
    let mut response = db
        .query("SELECT * FROM audit_checkpoints WHERE chain = $chain ORDER BY to_seq DESC LIMIT 1;")
        .bind(("chain", chain.name().to_string()))
        .await?
        .check()?;
    let rows: Vec<Value> = response.take(0)?;
    Ok(rows.first().and_then(Anchor::from_row))
}

/// One thing a verification found wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Failure {
    /// The row it concerns, when it concerns one.
    pub seq: Option<u64>,
    /// `hash`, `link`, `gap`, `head`, `signature`, `unknown_key`, `checkpoint`, `anchor`.
    pub rule: String,
    pub detail: String,
}

/// The outcome of verifying one chain.
#[derive(Debug, Clone, Serialize)]
pub struct ChainReport {
    pub chain: String,
    pub ok: bool,
    pub rows_checked: u64,
    pub head_seq: u64,
    /// Rows written but not sealed yet (normally seconds' worth).
    pub unsealed: u64,
    pub checkpoints_checked: u64,
    pub failures: Vec<Failure>,
    /// Failures beyond the ones listed.
    pub more_failures: u64,
}

struct Findings {
    list: Vec<Failure>,
    more: u64,
}

impl Findings {
    fn add(&mut self, seq: Option<u64>, rule: &str, detail: impl Into<String>) {
        if self.list.len() < MAX_FAILURES {
            self.list.push(Failure {
                seq,
                rule: rule.into(),
                detail: detail.into(),
            });
        } else {
            self.more += 1;
        }
    }
}

/// Recompute a chain and compare it with what is stored: every hash and link, that no number is
/// missing, that the chain ends where its head says, every checkpoint's signature, and (if given)
/// that an anchor kept elsewhere is part of it.
pub async fn verify(
    db: &Surreal<Client>,
    chain: Chain,
    anchor: Option<&Anchor>,
) -> Result<ChainReport, ChainError> {
    let head = head(db, chain).await?;
    let mut findings = Findings {
        list: Vec::new(),
        more: 0,
    };

    // The rows, in order.
    let (mut after, mut expected, mut prev) = (head.base_seq, head.base_seq + 1, head.base_hash.clone());
    let (mut checked, mut last_seq, mut last_hash) = (0u64, head.base_seq, head.base_hash.clone());
    'walk: loop {
        let mut response = db
            .query(format!(
                "SELECT * FROM {} WHERE chain_seq > $after ORDER BY chain_seq LIMIT $limit;",
                chain.name()
            ))
            .bind(("after", after))
            .bind(("limit", PAGE))
            .await?
            .check()?;
        let rows: Vec<Value> = response.take(0)?;
        if rows.is_empty() {
            break;
        }
        for row in &rows {
            let Some(seq) = row.get("chain_seq").and_then(Value::as_u64) else {
                continue;
            };
            if seq != expected {
                findings.add(
                    Some(expected),
                    "gap",
                    format!("expected row {expected}, found {seq}: rows were removed or added"),
                );
                break 'walk;
            }
            let stored_prev = row.get("prev_hash").and_then(Value::as_str).unwrap_or("");
            if stored_prev != prev {
                findings.add(Some(seq), "link", "the row does not link to the one before it");
                break 'walk;
            }
            let hash = match row_hash(chain, seq, &prev, row) {
                Ok(hash) => hash,
                Err(error) => {
                    findings.add(Some(seq), "hash", error.to_string());
                    break 'walk;
                }
            };
            if row.get("row_hash").and_then(Value::as_str) != Some(hash.as_str()) {
                findings.add(Some(seq), "hash", "the row's content does not match its hash");
                break 'walk;
            }
            checked += 1;
            (after, expected, prev) = (seq, seq + 1, hash.clone());
            (last_seq, last_hash) = (seq, hash);
        }
    }
    if findings.list.is_empty() && (last_seq != head.seq || last_hash != head.hash) {
        findings.add(
            Some(last_seq + 1),
            "head",
            format!(
                "the chain ends at row {last_seq} but its head says row {}: rows after it were removed",
                head.seq
            ),
        );
    }

    // Checkpoints: signed by a known key, and (the newest ones) matching the chain.
    let mut response = db
        .query(
            "SELECT * FROM audit_checkpoints WHERE chain = $chain ORDER BY to_seq; \
             SELECT * FROM audit_keys;",
        )
        .bind(("chain", chain.name().to_string()))
        .await?
        .check()?;
    let checkpoints: Vec<Value> = response.take(0)?;
    let keys: Vec<Value> = response.take(1)?;
    let public = |key_id: &str| -> Option<String> {
        keys.iter()
            .find(|row| row.get("key_id").and_then(Value::as_str) == Some(key_id))
            .and_then(|row| row.get("public_key").and_then(Value::as_str))
            .map(str::to_string)
    };
    let mut checkpoints_checked = 0;
    let first_matched = checkpoints.len().saturating_sub(CHECKPOINTS_MATCHED);
    for (index, row) in checkpoints.iter().enumerate() {
        let Some(point) = Anchor::from_row(row) else {
            continue;
        };
        checkpoints_checked += 1;
        match public(&point.key_id) {
            None => findings.add(
                Some(point.to_seq),
                "unknown_key",
                format!("checkpoint {} was signed by key {}, which is not on record", point.to_seq, point.key_id),
            ),
            Some(public) if !signature_ok(&public, &point.message(), &point.signature) => findings.add(
                Some(point.to_seq),
                "signature",
                format!("the signature of checkpoint {} is not valid", point.to_seq),
            ),
            Some(_) => {}
        }
        if index >= first_matched {
            check_point(db, chain, &head, &point, "checkpoint", &mut findings).await?;
        }
    }

    if let Some(anchor) = anchor {
        if anchor.chain != chain.name() {
            // An anchor of another chain says nothing about this one.
        } else {
            match public(&anchor.key_id) {
                None => findings.add(Some(anchor.to_seq), "anchor", "the anchor was signed by a key that is not on record"),
                Some(public) if !signature_ok(&public, &anchor.message(), &anchor.signature) => {
                    findings.add(Some(anchor.to_seq), "anchor", "the anchor's signature is not valid")
                }
                Some(_) => check_point(db, chain, &head, anchor, "anchor", &mut findings).await?,
            }
        }
    }

    let unsealed = {
        let mut response = db
            .query(format!("SELECT count() AS count FROM {} WHERE chain_seq = NONE GROUP ALL;", chain.name()))
            .await?
            .check()?;
        let counted: Vec<Value> = response.take(0)?;
        counted
            .first()
            .and_then(|row| row.get("count"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    Ok(ChainReport {
        chain: chain.name().to_string(),
        ok: findings.list.is_empty(),
        rows_checked: checked,
        head_seq: head.seq,
        unsealed,
        checkpoints_checked,
        failures: findings.list,
        more_failures: findings.more,
    })
}

/// Whether the chain agrees with a checkpoint or anchor: the row it names is still there with that
/// hash (or it was purged and the chain's base is that very point), and the chain is not shorter.
async fn check_point(
    db: &Surreal<Client>,
    chain: Chain,
    head: &Head,
    point: &Anchor,
    rule: &str,
    findings: &mut Findings,
) -> Result<(), ChainError> {
    if point.to_seq > head.seq {
        findings.add(
            Some(point.to_seq),
            rule,
            format!("the {rule} is at row {} but the chain ends at row {}: the chain is shorter", point.to_seq, head.seq),
        );
        return Ok(());
    }
    if point.to_seq < head.base_seq {
        return Ok(()); // purged since; its signature is all that remains to check
    }
    if point.to_seq == head.base_seq {
        if point.head_hash != head.base_hash {
            findings.add(Some(point.to_seq), rule, "the chain's starting point does not match the checkpoint");
        }
        return Ok(());
    }
    let mut response = db
        .query(format!("SELECT VALUE row_hash FROM {} WHERE chain_seq = $seq LIMIT 1;", chain.name()))
        .bind(("seq", point.to_seq))
        .await?
        .check()?;
    let found: Vec<Value> = response.take(0)?;
    match found.first().and_then(Value::as_str) {
        Some(hash) if hash == point.head_hash => {}
        Some(_) => findings.add(Some(point.to_seq), rule, format!("row {} is not what the {rule} says it was", point.to_seq)),
        None => findings.add(Some(point.to_seq), rule, format!("row {} named by the {rule} is missing", point.to_seq)),
    }
    Ok(())
}

/// Remove the sealed rows older than `days`, in whole: the longest prefix of the chain that ends at
/// a checkpoint whose row is older than that. The chain then starts from that checkpoint's hash, so
/// it still verifies. Unsealed rows are never removed. Returns how many rows went.
pub async fn purge_before(db: &Surreal<Client>, chain: Chain, days: u32) -> Result<u64, ChainError> {
    let head = head(db, chain).await?;
    let mut response = db
        .query("SELECT to_seq, head_hash FROM audit_checkpoints WHERE chain = $chain AND to_seq > $base ORDER BY to_seq;")
        .bind(("chain", chain.name().to_string()))
        .bind(("base", head.base_seq))
        .await?
        .check()?;
    let points: Vec<Value> = response.take(0)?;
    if points.is_empty() {
        return Ok(0);
    }
    let old_enough = |to_seq: u64| {
        let table = chain.name();
        async move {
            let mut response = db
                .query(format!(
                    "LET $cutoff = time::now() - <duration> string::concat($days, 'd'); \
                     RETURN array::len(SELECT * FROM {table} WHERE chain_seq = $seq AND date_created < $cutoff);"
                ))
                .bind(("days", i64::from(days)))
                .bind(("seq", to_seq))
                .await?
                .check()?;
            let found: Option<u64> = response.take(1)?;
            Ok::<bool, ChainError>(found.unwrap_or(0) > 0)
        }
    };
    // The newest checkpoint whose row is old enough; checkpoints get younger along the chain, so
    // this is a binary search.
    let (mut low, mut high, mut best) = (0usize, points.len(), None);
    while low < high {
        let middle = (low + high) / 2;
        let to_seq = points[middle].get("to_seq").and_then(Value::as_u64).unwrap_or(0);
        if old_enough(to_seq).await? {
            best = Some(middle);
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    let Some(best) = best else {
        return Ok(0);
    };
    let to_seq = points[best].get("to_seq").and_then(Value::as_u64).unwrap_or(0);
    let hash = points[best].get("head_hash").and_then(Value::as_str).unwrap_or("").to_string();
    let mut response = db
        .query(format!(
            "BEGIN TRANSACTION; \
             LET $gone = (SELECT VALUE id FROM {table} WHERE chain_seq != NONE AND chain_seq <= $to); \
             DELETE {table} WHERE chain_seq != NONE AND chain_seq <= $to; \
             UPDATE audit_chain_head SET base_seq = $to, base_hash = $hash WHERE chain = $chain; \
             RETURN array::len($gone); \
             COMMIT TRANSACTION;",
            table = chain.name()
        ))
        .bind(("chain", chain.name().to_string()))
        .bind(("to", to_seq))
        .bind(("hash", hash))
        .await?
        .check()?;
    let removed: Option<u64> = response.take(4)?;
    Ok(removed.unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access_row() -> Value {
        json!({
            "id": "data_access:abc",
            "request_id": "r1",
            "actor_type": "user",
            "actor_id": "users:ann",
            "plugin": "crm",
            "function_name": "add_lead",
            "model": "lead",
            "table_name": "mdl_x",
            "operation": "create",
            "record_ids": ["mdl_x:1", "mdl_x:2"],
            "record_count": 2,
            "statement": null,
            "ip": "10.0.0.1",
            "date_created": "2026-10-16T21:33:28.123456789Z"
        })
    }

    #[test]
    fn the_canonical_form_is_fixed() {
        // Written out in full: any change to it breaks every verifier ever written.
        assert_eq!(
            canonical(Chain::DataAccess, 7, "ab12", &access_row()).unwrap_or_default(),
            "aether-audit-v1|data_access|7|ab12|1792186408123456789|=r1|=user|=users:ann|=crm|=add_lead|\
             =lead|=mdl_x|=create|[=mdl_x:1,=mdl_x:2]|=2|-|=10.0.0.1"
        );
    }

    #[test]
    fn a_row_hashes_to_a_fixed_value() {
        let hash = row_hash(Chain::DataAccess, 7, "ab12", &access_row()).unwrap_or_default();
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, row_hash(Chain::DataAccess, 7, "ab12", &access_row()).unwrap_or_default());
    }

    #[test]
    fn any_change_to_a_row_or_its_place_changes_its_hash() {
        let base = row_hash(Chain::DataAccess, 7, "ab12", &access_row()).unwrap_or_default();
        let mut changed = vec![
            row_hash(Chain::DataAccess, 8, "ab12", &access_row()).unwrap_or_default(),
            row_hash(Chain::DataAccess, 7, "ab13", &access_row()).unwrap_or_default(),
            row_hash(Chain::PluginCalls, 7, "ab12", &access_row()).unwrap_or_default(),
        ];
        for (field, value) in [
            ("actor_id", json!("users:bob")),
            ("operation", json!("read")),
            ("record_ids", json!(["mdl_x:1"])),
            ("record_count", json!(3)),
            ("statement", json!("x")),
            ("ip", json!(null)),
            ("date_created", json!("2026-10-16T21:33:28.123456790Z")),
        ] {
            let mut row = access_row();
            row[field] = value;
            changed.push(row_hash(Chain::DataAccess, 7, "ab12", &row).unwrap_or_default());
        }
        for hash in &changed {
            assert_ne!(hash, &base);
        }
        changed.sort();
        changed.dedup();
        assert_eq!(changed.len(), 10, "and each change is its own hash");
    }

    #[test]
    fn missing_is_not_the_same_as_empty_and_separators_cannot_be_forged() {
        let mut none = access_row();
        none["ip"] = json!(null);
        let mut empty = access_row();
        empty["ip"] = json!("");
        assert_ne!(
            canonical(Chain::DataAccess, 1, "", &none).unwrap_or_default(),
            canonical(Chain::DataAccess, 1, "", &empty).unwrap_or_default()
        );
        // Two list items that contain a comma are not one item that contains it.
        let mut two = access_row();
        two["record_ids"] = json!(["a", "b"]);
        let mut one = access_row();
        one["record_ids"] = json!(["a,=b"]);
        assert_ne!(
            canonical(Chain::DataAccess, 1, "", &two).unwrap_or_default(),
            canonical(Chain::DataAccess, 1, "", &one).unwrap_or_default()
        );
        // A `|` in a value cannot pass for the end of the field.
        let mut pipe = access_row();
        pipe["plugin"] = json!("crm|add_lead");
        assert!(canonical(Chain::DataAccess, 1, "", &pipe).unwrap_or_default().contains("=crm\\|add_lead"));
    }

    #[test]
    fn a_row_without_a_time_cannot_be_hashed() {
        let mut row = access_row();
        row.as_object_mut().map(|map| map.remove("date_created"));
        assert!(canonical(Chain::DataAccess, 1, "", &row).is_err());
    }

    #[test]
    fn checkpoints_are_signed_and_forgeries_fail() {
        let key = AuditKey::from_seed([7u8; 32]);
        let message = checkpoint_message("org_acme", "data_access", 120, "abcd", 42);
        let signature = key.sign(&message);
        assert!(signature_ok(&key.public_hex(), &message, &signature));
        let other = checkpoint_message("org_acme", "data_access", 121, "abcd", 42);
        assert!(!signature_ok(&key.public_hex(), &other, &signature), "another point");
        let stranger = AuditKey::from_seed([8u8; 32]);
        assert!(!signature_ok(&stranger.public_hex(), &message, &signature), "another key");
        assert!(!signature_ok("zz", &message, &signature));
        assert!(!signature_ok(&key.public_hex(), &message, "00"));
        assert_ne!(key.key_id(), stranger.key_id());
        assert_eq!(key.key_id().len(), 16);
    }

    #[test]
    fn a_key_file_is_made_once_and_read_back() {
        let dir = std::env::temp_dir().join(format!("aether-audit-key-{}", std::process::id()));
        let path = dir.join("conf").join("audit.key");
        let first = AuditKey::load_or_create(&path).expect("created");
        let again = AuditKey::load_or_create(&path).expect("read back");
        assert_eq!(first.public_hex(), again.public_hex());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).map(|m| m.permissions().mode() & 0o777).unwrap_or(0);
            assert_eq!(mode, 0o600);
        }
        std::fs::write(&path, "not hex").expect("write");
        assert!(AuditKey::load_or_create(&path).is_err(), "a damaged key is not replaced silently");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_chain_names_its_fields_and_round_trips_its_name() {
        for chain in Chain::ALL {
            assert_eq!(Chain::parse(chain.name()), Some(chain));
            assert!(chain.fields().contains(&"request_id"));
        }
        assert_eq!(Chain::parse("nope"), None);
    }
}
