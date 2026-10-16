//! What an OCR / document-parsing engine is, and what the kernel asks of one.
//!
//! A plugin asks the kernel to *parse a stored file* (`ocr::parse`) and gets back one normalised
//! result, whichever engine produced it. An engine is a bridge under `bridges/document_parsing/`
//! (LiteParse first); it knows how to turn one file on disk into an [`OcrResult`] and nothing about
//! plugins, organizations or the database. This crate holds what every engine and the kernel
//! share: the options, the result shape, the limits, and the temporary file the kernel hands an
//! engine.
//!
//! Limits are deliberate: a parse is CPU and memory heavy, runs on untrusted files, and a plugin
//! call may only wait a few seconds. See `docs/architecture/ocr.md`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The settings that bound a parse. Every one is a kernel setting (`ocr.*`, global with an organization
/// override); these are the defaults, and `HARD_*` are what no setting can exceed.
pub const DEFAULT_MAX_INPUT_MB: u64 = 25;
pub const DEFAULT_MAX_PAGES: u32 = 100;
pub const DEFAULT_MAX_TEXT_KB: u64 = 2048;
pub const DEFAULT_SYNC_CEILING_SECS: u64 = 8;
pub const DEFAULT_MAX_CONCURRENT: usize = 2;
pub const DEFAULT_MAX_QUEUED: u64 = 50;
pub const HARD_MAX_INPUT_MB: u64 = 100;
pub const HARD_MAX_PAGES: u32 = 1000;
pub const HARD_MAX_TEXT_KB: u64 = 16 * 1024;
/// A plugin call is cut off after ten seconds, so a command that waits for the engine cannot be told to
/// wait longer than this.
pub const HARD_SYNC_CEILING_SECS: u64 = 9;
pub const HARD_MAX_CONCURRENT: usize = 8;
pub const HARD_MAX_QUEUED: u64 = 1000;

/// The setting keys, in one place so the seed file, the kernel and the docs agree.
pub mod keys {
    pub const ENABLED: &str = "ocr.enabled";
    pub const MAX_INPUT_MB: &str = "ocr.max_input_mb";
    pub const MAX_PAGES: &str = "ocr.max_pages";
    pub const MAX_TEXT_KB: &str = "ocr.max_text_kb";
    pub const SYNC_CEILING_SECS: &str = "ocr.sync_ceiling_secs";
    pub const MAX_CONCURRENT: &str = "ocr.max_concurrent";
    pub const ENGINE: &str = "ocr.engine";
    pub const MAX_QUEUED: &str = "ocr.max_queued";
    pub const ALL: &[&str] =
        &[ENABLED, MAX_INPUT_MB, MAX_PAGES, MAX_TEXT_KB, SYNC_CEILING_SECS, MAX_CONCURRENT, ENGINE, MAX_QUEUED];
}

/// The limits in force for one organization, from its settings and clamped to the hard caps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    pub enabled: bool,
    pub max_input_bytes: usize,
    pub max_pages: u32,
    pub max_text_bytes: usize,
    pub sync_ceiling_secs: u64,
    pub max_concurrent: usize,
    pub engine: String,
    /// Parses waiting or running for the organization; more are refused until some finish.
    pub max_queued: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self::from_settings(|_| None)
    }
}

impl Limits {
    /// Build from a lookup of setting values (numbers may be stored as numbers or as text). An unreadable
    /// or out-of-range value falls back to the default, then is clamped to the hard cap.
    pub fn from_settings(get: impl Fn(&str) -> Option<serde_json::Value>) -> Self {
        let number = |key: &str, default: u64, low: u64, high: u64| -> u64 {
            get(key)
                .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())))
                .unwrap_or(default)
                .clamp(low, high)
        };
        let flag = |key: &str, default: bool| -> bool {
            get(key)
                .and_then(|v| v.as_bool().or_else(|| v.as_str().map(|s| matches!(s.trim(), "true" | "1" | "yes" | "on"))))
                .unwrap_or(default)
        };
        Self {
            enabled: flag(keys::ENABLED, true),
            max_input_bytes: (number(keys::MAX_INPUT_MB, DEFAULT_MAX_INPUT_MB, 1, HARD_MAX_INPUT_MB) * 1024 * 1024) as usize,
            max_pages: number(keys::MAX_PAGES, u64::from(DEFAULT_MAX_PAGES), 1, u64::from(HARD_MAX_PAGES)) as u32,
            max_text_bytes: (number(keys::MAX_TEXT_KB, DEFAULT_MAX_TEXT_KB, 16, HARD_MAX_TEXT_KB) * 1024) as usize,
            sync_ceiling_secs: number(keys::SYNC_CEILING_SECS, DEFAULT_SYNC_CEILING_SECS, 1, HARD_SYNC_CEILING_SECS),
            max_concurrent: number(keys::MAX_CONCURRENT, DEFAULT_MAX_CONCURRENT as u64, 1, HARD_MAX_CONCURRENT as u64) as usize,
            engine: get(keys::ENGINE)
                .and_then(|v| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "liteparse".to_string()),
            max_queued: number(keys::MAX_QUEUED, DEFAULT_MAX_QUEUED, 1, HARD_MAX_QUEUED),
        }
    }
}

/// The folder the kernel puts files in for an engine; an engine refuses any path outside it.
pub fn work_root() -> PathBuf {
    std::env::temp_dir().join("aether-ocr")
}

/// What to parse and how.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Options {
    /// Tesseract language code(s), `eng` or `eng+fra`.
    #[serde(default = "default_language")]
    pub language: String,
    /// Pages to read, `"1-5,8"`; all (up to the `ocr.max_pages` setting) when left out.
    #[serde(default)]
    pub pages: Option<String>,
    /// Run OCR on pages that have no text layer (on by default).
    #[serde(default = "yes")]
    pub ocr: bool,
}

fn default_language() -> String {
    "eng".into()
}

fn yes() -> bool {
    true
}

impl Default for Options {
    fn default() -> Self {
        Self { language: default_language(), pages: None, ocr: true }
    }
}

impl Options {
    /// Refuse anything an engine should not be handed on its command line.
    pub fn validate(&self) -> Result<(), String> {
        let ok = !self.language.is_empty()
            && self.language.len() <= 40
            && self.language.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '+');
        if !ok {
            return Err("`language` is Tesseract codes such as `eng` or `eng+fra`".into());
        }
        if let Some(pages) = &self.pages {
            let ok = !pages.is_empty()
                && pages.len() <= 100
                && pages.chars().all(|c| c.is_ascii_digit() || c == ',' || c == '-')
                && !pages.starts_with(['-', ','])
                && !pages.ends_with(['-', ',']);
            if !ok {
                return Err("`pages` is a list such as `1-5,8`".into());
            }
        }
        Ok(())
    }

    /// A stable text of the options, for cache keys.
    pub fn fingerprint(&self) -> String {
        format!("{}|{}|{}", self.language, self.pages.as_deref().unwrap_or("*"), self.ocr)
    }
}

/// One piece of a page: a heading, a paragraph, a table, a line of OCR text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub kind: String,
    pub text: String,
    /// `[x1, y1, x2, y2]` in points from the top-left of the page (72 per inch), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<[f64; 4]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub page: u32,
    pub width: f64,
    pub height: f64,
    pub text: String,
    pub blocks: Vec<Block>,
    /// Mean OCR confidence (0 to 1) where the page was read by OCR.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

/// The one result shape every engine produces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcrResult {
    pub text: String,
    pub pages: Vec<Page>,
    pub total_pages: u32,
    pub language: String,
    pub engine: String,
    pub ocr_used: bool,
    pub truncated: bool,
    pub warnings: Vec<String>,
}

impl OcrResult {
    /// Cut the text (and each page's) to `limit` bytes on a character boundary.
    pub fn limit_text(&mut self, limit: usize) {
        fn cut(text: &mut String, limit: usize) -> bool {
            if text.len() <= limit {
                return false;
            }
            let mut end = limit;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            true
        }
        let mut budget = limit;
        let mut cut_any = cut(&mut self.text, budget);
        for page in &mut self.pages {
            let allowed = budget.min(page.text.len());
            cut_any |= cut(&mut page.text, allowed);
            budget = budget.saturating_sub(page.text.len());
            if budget == 0 {
                page.blocks.clear();
            }
        }
        if cut_any {
            self.truncated = true;
            self.warnings.push("the text is longer than the limit and was cut".into());
        }
    }
}

/// The key a result is kept under. A media key is never reused for different bytes, so the same key,
/// engine and options always give the same result and the file need not be read to know it.
pub fn cache_key(media_key: &str, engine: &str, options: &Options) -> String {
    let mut hasher = Sha256::new();
    hasher.update(media_key.as_bytes());
    hasher.update([0]);
    hasher.update(engine.as_bytes());
    hasher.update([0]);
    hasher.update(options.fingerprint().as_bytes());
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// The file extension an engine needs to recognise a file, from its name when it has a known one, else
/// from its bytes. `None` for what is not parsed.
pub fn extension_for(bytes: &[u8], name: Option<&str>) -> Option<&'static str> {
    const KNOWN: &[&str] = &[
        "pdf", "png", "jpg", "jpeg", "gif", "webp", "tif", "tiff", "bmp", "docx", "xlsx", "pptx", "odt", "ods", "odp",
        "rtf", "csv", "tsv", "txt",
    ];
    if let Some(ext) = name.and_then(|n| n.rsplit('.').next()).map(str::to_ascii_lowercase) {
        if let Some(known) = KNOWN.iter().find(|k| **k == ext) {
            return Some(known);
        }
    }
    if bytes.starts_with(b"%PDF-") {
        return Some("pdf");
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if bytes.starts_with(b"GIF8") {
        return Some("gif");
    }
    if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    if bytes.starts_with(b"PK\x03\x04") {
        // An Office file is a zip; its folders say which.
        let head = &bytes[..bytes.len().min(64 * 1024)];
        let has = |needle: &[u8]| head.windows(needle.len()).any(|w| w == needle);
        if has(b"word/") {
            return Some("docx");
        }
        if has(b"xl/") {
            return Some("xlsx");
        }
        if has(b"ppt/") {
            return Some("pptx");
        }
    }
    None
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A file written for an engine, removed (with its folder) when dropped.
#[derive(Debug)]
pub struct TempInput {
    dir: PathBuf,
    path: PathBuf,
}

impl TempInput {
    pub fn create(bytes: &[u8], extension: &str) -> std::io::Result<Self> {
        let id = format!(
            "{:x}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())
        );
        let dir = work_root().join(id);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("input.{extension}"));
        std::fs::write(&path, bytes)?;
        Ok(Self { dir, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempInput {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Whether `path` is inside the kernel's work folder (engines refuse anything else).
pub fn is_work_path(path: &Path) -> bool {
    match (path.canonicalize(), work_root().canonicalize()) {
        (Ok(p), Ok(root)) => p.starts_with(root),
        _ => false,
    }
}

/// Counts the parses running in this process, so the `ocr.max_concurrent` setting can change without a restart.
pub struct Gate {
    running: std::sync::atomic::AtomicUsize,
}

/// Held while a parse runs; releases its place when dropped.
pub struct Permit(&'static Gate);

impl Drop for Permit {
    fn drop(&mut self) {
        self.0.running.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Gate {
    /// A place for one more parse, waiting up to `wait` for one to free up; `None` when none did.
    pub async fn acquire(&'static self, limit: usize, wait: std::time::Duration) -> Option<Permit> {
        let started = std::time::Instant::now();
        loop {
            let now = self.running.load(Ordering::Acquire);
            if now < limit && self.running.compare_exchange(now, now + 1, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                return Some(Permit(self));
            }
            if started.elapsed() >= wait {
                return None;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    pub fn running(&self) -> usize {
        self.running.load(Ordering::Acquire)
    }
}

pub fn gate() -> &'static Gate {
    static GATE: Gate = Gate { running: std::sync::atomic::AtomicUsize::new(0) };
    &GATE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_refuse_what_should_not_reach_a_command_line() {
        assert!(Options::default().validate().is_ok());
        assert!(Options { language: "eng+fra".into(), pages: Some("1-3,7".into()), ocr: true }.validate().is_ok());
        assert!(Options { language: "eng; rm".into(), ..Options::default() }.validate().is_err());
        assert!(Options { pages: Some("1 2".into()), ..Options::default() }.validate().is_err());
        assert!(Options { pages: Some("-1".into()), ..Options::default() }.validate().is_err());
        assert!(Options { language: String::new(), ..Options::default() }.validate().is_err());
    }

    #[test]
    fn the_cache_key_changes_with_file_engine_and_options() {
        let o = Options::default();
        let a = cache_key("media/2026/10/one.pdf", "liteparse", &o);
        assert_eq!(a, cache_key("media/2026/10/one.pdf", "liteparse", &o));
        assert_ne!(a, cache_key("media/2026/10/two.pdf", "liteparse", &o));
        assert_ne!(a, cache_key("media/2026/10/one.pdf", "other", &o));
        assert_ne!(a, cache_key("media/2026/10/one.pdf", "liteparse", &Options { ocr: false, ..o }));
    }

    #[test]
    fn the_extension_comes_from_the_name_or_the_bytes() {
        assert_eq!(extension_for(b"junk", Some("Scan.PDF")), Some("pdf"));
        assert_eq!(extension_for(b"%PDF-1.7", None), Some("pdf"));
        assert_eq!(extension_for(b"\x89PNG\r\n\x1a\nxx", Some("noext")), Some("png"));
        assert_eq!(extension_for(b"PK\x03\x04....word/document.xml", None), Some("docx"));
        assert_eq!(extension_for(b"PK\x03\x04....xl/workbook.xml", None), Some("xlsx"));
        assert_eq!(extension_for(b"plain", Some("x.exe")), None);
    }

    #[test]
    fn text_is_cut_on_a_character_boundary() {
        let mut r = OcrResult {
            text: "é".repeat(5000),
            pages: vec![],
            total_pages: 0,
            language: "eng".into(),
            engine: "t".into(),
            ocr_used: false,
            truncated: false,
            warnings: vec![],
        };
        r.limit_text(1000);
        assert!(r.truncated && r.text.len() <= 1000 && r.text.is_char_boundary(r.text.len()));
    }

    #[test]
    fn limits_come_from_settings_and_never_pass_the_hard_caps() {
        let d = Limits::default();
        assert!(d.enabled && d.max_pages == 100 && d.sync_ceiling_secs == 8 && d.max_concurrent == 2 && d.engine == "liteparse" && d.max_queued == 50);
        let set = |k: &str| -> Option<serde_json::Value> {
            match k {
                "ocr.enabled" => Some(serde_json::json!(false)),
                "ocr.max_pages" => Some(serde_json::json!(99999)),
                "ocr.max_input_mb" => Some(serde_json::json!("50")),
                "ocr.sync_ceiling_secs" => Some(serde_json::json!(60)),
                "ocr.max_concurrent" => Some(serde_json::json!("not a number")),
                "ocr.engine" => Some(serde_json::json!("  tesseract ")),
                _ => None,
            }
        };
        let l = Limits::from_settings(set);
        assert!(!l.enabled);
        assert_eq!(l.max_pages, HARD_MAX_PAGES);
        assert_eq!(l.max_input_bytes, 50 * 1024 * 1024);
        assert_eq!(l.sync_ceiling_secs, HARD_SYNC_CEILING_SECS);
        assert_eq!(l.max_concurrent, DEFAULT_MAX_CONCURRENT);
        assert_eq!(l.engine, "tesseract");
    }

    #[tokio::test]
    async fn the_gate_holds_to_its_limit_and_releases() {
        let g = gate();
        let a = g.acquire(1, std::time::Duration::from_millis(10)).await;
        assert!(a.is_some());
        assert!(g.acquire(1, std::time::Duration::from_millis(60)).await.is_none());
        drop(a);
        assert!(g.acquire(1, std::time::Duration::from_millis(10)).await.is_some());
    }

    #[test]
    fn a_temp_file_is_inside_the_work_folder_and_removed_after() {
        let path = {
            let t = TempInput::create(b"%PDF-1.4", "pdf").unwrap();
            assert!(is_work_path(t.path()));
            assert!(t.path().exists());
            t.path().to_path_buf()
        };
        assert!(!path.exists());
        assert!(!is_work_path(Path::new("/etc/passwd")));
    }
}
