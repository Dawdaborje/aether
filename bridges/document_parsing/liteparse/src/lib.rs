//! LiteParse: document parsing and OCR. <https://github.com/run-llama/liteparse>
//!
//! LiteParse turns a PDF, an Office file or an image into text with positions, running Tesseract
//! (or an HTTP OCR server) on pages that have no text layer. This bridge runs its command line as a
//! **child process**: PDFium parses untrusted uploads, and a crash or hang in it must never reach the
//! kernel. The process gets its own folder, an almost empty environment, a time limit and a cap on
//! what it may print; its whole process group is killed when the time is up.
//!
//! The binary is called `liteparse` here; on many systems `lit` is another program (LLVM's test
//! runner), so the bridge verifies the binary before it trusts it.
//!
//! Actions:
//! * `parse { path, language?, pages?, ocr? }`, with `path` a file the kernel put in its OCR work folder,
//!   answers the normalised result of [`aether_ocr::OcrResult`].
//! * `status {}` answers `{ found, version, identity_ok, binary }`.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use aether_communication::{Action, ActionBridge, ConfigError, Field, SendError, Spec, Values};
use aether_ocr::{Block, OcrResult, Options, Page};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub const SPEC: Spec = Spec {
    key: "liteparse",
    label: "LiteParse",
    account: &[Field {
        name: "ocr_server_token",
        label: "Token for the HTTP OCR server (when one is used)",
        secret: true,
        required: false,
        default: None,
    }],
    options: &[
        Field { name: "binary_path", label: "LiteParse program", secret: false, required: false, default: Some("liteparse") },
        Field { name: "default_language", label: "OCR language", secret: false, required: false, default: Some("eng") },
        Field { name: "tessdata_path", label: "Tesseract language data folder", secret: false, required: false, default: None },
        Field { name: "ocr_server_url", label: "HTTP OCR server (instead of Tesseract)", secret: false, required: false, default: None },
        Field { name: "dpi", label: "Rendering resolution for OCR", secret: false, required: false, default: Some("150") },
        Field { name: "max_pages", label: "Most pages read per file", secret: false, required: false, default: Some("100") },
        Field { name: "timeout_secs", label: "Time allowed for one file", secret: false, required: false, default: Some("60") },
    ],
};

pub const ACTIONS: &[Action] = &[
    Action { name: "parse", help: "{ path, language?, pages?, ocr?, max_pages?, max_text_bytes?, timeout_secs? } -> { text, pages: [{ page, width, height, text, blocks, confidence? }], total_pages, language, engine, ocr_used, truncated, warnings }" },
    Action { name: "status", help: "{} -> { found, version, identity_ok, binary }" },
];

/// Most the program may print on standard output.
const MAX_STDOUT: u64 = 64 * 1024 * 1024;
/// Most of its messages kept.
const MAX_STDERR: u64 = 32 * 1024;

pub struct LiteParse {
    binary: String,
    language: String,
    tessdata: Option<String>,
    server_url: Option<String>,
    server_token: Option<String>,
    dpi: u32,
    max_pages: u32,
    timeout: Duration,
}

fn setting<'a>(values: &'a Values, name: &str) -> Option<&'a str> {
    values.get(name).map(|v| v.trim()).filter(|v| !v.is_empty())
}

impl LiteParse {
    pub fn new(values: &Values) -> Result<Self, ConfigError> {
        let number = |name: &str, default: u32, low: u32, high: u32| -> u32 {
            setting(values, name).and_then(|v| v.parse::<u32>().ok()).filter(|n| (low..=high).contains(n)).unwrap_or(default)
        };
        Ok(Self {
            binary: setting(values, "binary_path").unwrap_or("liteparse").to_string(),
            language: setting(values, "default_language").unwrap_or("eng").to_string(),
            tessdata: setting(values, "tessdata_path").map(str::to_string),
            server_url: setting(values, "ocr_server_url").map(str::to_string),
            server_token: setting(values, "ocr_server_token").map(str::to_string),
            dpi: number("dpi", 150, 72, 400),
            max_pages: number("max_pages", 100, 1, aether_ocr::HARD_MAX_PAGES),
            timeout: Duration::from_secs(u64::from(number("timeout_secs", 60, 5, 600))),
        })
    }

    fn command(&self, args: &[String], cwd: &std::path::Path) -> Command {
        let mut command = Command::new(&self.binary);
        command.args(args).current_dir(cwd).env_clear().stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        // Nothing of the kernel's environment goes in. LibreOffice (for Office files) needs a writable home,
        // and is told not to use a GPU: on some machines its OpenCL probe aborts the whole conversion.
        command
            .env("PATH", "/usr/local/bin:/usr/bin:/bin")
            .env("HOME", aether_ocr::work_root().join("home"))
            .env("SAL_DISABLE_OPENCL", "1")
            .env("SAL_USE_VCLPLUGIN", "svp")
            .env("LANG", "C.UTF-8");
        if let Some(path) = &self.tessdata {
            command.env("TESSDATA_PREFIX", path);
        }
        #[cfg(unix)]
        command.process_group(0);
        command
    }

    /// Run the program with a time limit; kill its whole process group when the time is up.
    async fn run(&self, args: &[String], cwd: &std::path::Path, limit: Duration) -> Result<(String, String), SendError> {
        let mut child = self
            .command(args, cwd)
            .spawn()
            .map_err(|e| SendError::permanent(format!("LiteParse could not be started from `{}` ({e}): set the program's path in the settings", self.binary)))?;
        let pid = child.id();
        let (Some(mut stdout), Some(mut stderr)) = (child.stdout.take(), child.stderr.take()) else {
            return Err(SendError::permanent("LiteParse's output could not be read"));
        };
        let work = async {
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let mut limited_out = (&mut stdout).take(MAX_STDOUT);
            let mut limited_err = (&mut stderr).take(MAX_STDERR);
            let (a, b) = tokio::join!(limited_out.read_to_end(&mut out), limited_err.read_to_end(&mut err));
            a.and(b).map_err(|e| SendError::transient(format!("LiteParse's output could not be read ({e})")))?;
            let status = child.wait().await.map_err(|e| SendError::transient(format!("LiteParse did not finish ({e})")))?;
            Ok::<_, SendError>((status, out, err))
        };
        match tokio::time::timeout(limit, work).await {
            Err(_) => {
                kill_group(pid);
                Err(SendError::transient(format!("LiteParse did not finish within {} seconds", limit.as_secs())))
            }
            Ok(Err(e)) => {
                kill_group(pid);
                Err(e)
            }
            Ok(Ok((status, out, err))) => {
                let out = String::from_utf8_lossy(&out).into_owned();
                let err = String::from_utf8_lossy(&err).into_owned();
                if status.success() {
                    Ok((out, err))
                } else {
                    let reason = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("it reported no reason");
                    // Cut: a crash dump can run to pages and name paths.
                    Err(SendError::permanent(format!("LiteParse could not read the file: {}", reason.chars().take(300).collect::<String>())))
                }
            }
        }
    }

    async fn status(&self) -> Value {
        let scratch = aether_ocr::work_root();
        let _ = std::fs::create_dir_all(&scratch);
        let version = self.run(&["--version".into()], &scratch, Duration::from_secs(10)).await;
        let help = self.run(&["parse".into(), "--help".into()], &scratch, Duration::from_secs(10)).await;
        let found = version.is_ok();
        // LLVM's `lit` also answers `--version`; only this program has a `parse` command that takes `--ocr-language`.
        let identity_ok = help.as_ref().is_ok_and(|(out, _)| out.contains("--ocr-language"));
        json!({
            "found": found,
            "version": version.ok().map(|(o, _)| o.trim().to_string()),
            "identity_ok": identity_ok,
            "binary": self.binary,
        })
    }

    async fn parse(&self, params: &Value) -> Result<Value, SendError> {
        let path = params["path"].as_str().filter(|p| !p.is_empty()).ok_or_else(|| SendError::permanent("`path` is required"))?;
        let path = PathBuf::from(path);
        if !aether_ocr::is_work_path(&path) {
            return Err(SendError::permanent("`path` must be a file the kernel prepared"));
        }
        let options = Options {
            language: params["language"].as_str().map_or_else(|| self.language.clone(), str::to_string),
            pages: params["pages"].as_str().map(str::to_string),
            ocr: params["ocr"].as_bool().unwrap_or(true),
        };
        options.validate().map_err(SendError::permanent)?;
        let folder = path.parent().map(PathBuf::from).unwrap_or_else(aether_ocr::work_root);
        let _ = std::fs::create_dir_all(aether_ocr::work_root().join("home"));
        let mut args: Vec<String> = vec![
            "parse".into(),
            path.to_string_lossy().into_owned(),
            "--format".into(),
            "json".into(),
            "--extract-blocks".into(),
            "--image-mode".into(),
            "off".into(),
            "--num-workers".into(),
            "1".into(),
            "--dpi".into(),
            self.dpi.to_string(),
            "--max-pages".into(),
            // The caller (the kernel, from its `ocr.max_pages` setting) may ask for fewer pages than this
            // bridge's own setting allows, never more.
            params["max_pages"].as_u64().map_or(self.max_pages, |n| (n.min(u64::from(self.max_pages)).max(1)) as u32).to_string(),
            "--ocr-language".into(),
            options.language.clone(),
        ];
        if !options.ocr {
            args.push("--no-ocr".into());
        }
        if let Some(pages) = &options.pages {
            args.push("--target-pages".into());
            args.push(pages.clone());
        }
        if let Some(url) = &self.server_url {
            args.push("--ocr-server-url".into());
            args.push(url.clone());
            if let Some(token) = &self.server_token {
                args.push("--ocr-server-header".into());
                args.push(format!("Authorization: Bearer {token}"));
            }
        }
        // The caller may ask for less time than the setting allows (never more).
        let limit = params["timeout_secs"]
            .as_u64()
            .map_or(self.timeout, |secs| Duration::from_secs(secs.clamp(1, 600)).min(self.timeout));
        let (stdout, _log) = self.run(&args, &folder, limit).await?;
        let document: Value = serde_json::from_str(&stdout).map_err(|_| SendError::permanent("LiteParse's answer was not JSON"))?;
        let mut result = normalise(&document, &options.language, "liteparse");
        let text_limit = params["max_text_bytes"].as_u64().map_or(2 * 1024 * 1024, |n| n as usize);
        result.limit_text(text_limit);
        serde_json::to_value(result).map_err(|e| SendError::permanent(e.to_string()))
    }
}

#[cfg(unix)]
fn kill_group(pid: Option<u32>) {
    if let Some(pid) = pid {
        // The program starts LibreOffice and others; take the whole group down, not only the parent.
        let _ = std::process::Command::new("kill").args(["-KILL", &format!("-{pid}")]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
}

#[cfg(not(unix))]
fn kill_group(_: Option<u32>) {}

fn number(value: &Value, name: &str) -> f64 {
    value[name].as_f64().unwrap_or(0.0)
}

/// LiteParse's boxes are `{ x, y, width, height }`; the shared shape is corners.
fn corners(bbox: &Value) -> Option<[f64; 4]> {
    bbox.is_object().then(|| {
        let (x, y) = (number(bbox, "x"), number(bbox, "y"));
        [x, y, x + number(bbox, "width"), y + number(bbox, "height")]
    })
}

/// A block carries `text` (after OCR) or `lines` (a text layer).
fn block_text(block: &Value) -> String {
    match block["text"].as_str() {
        Some(text) => text.to_string(),
        None => block["lines"]
            .as_array()
            .map(|lines| lines.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n"))
            .unwrap_or_default(),
    }
}

/// LiteParse's JSON in the shape every engine produces.
pub fn normalise(document: &Value, language: &str, engine: &str) -> OcrResult {
    let mut pages = Vec::new();
    let mut ocr_used = false;
    for page in document["pages"].as_array().cloned().unwrap_or_default() {
        let items = page["text_items"].as_array().cloned().unwrap_or_default();
        let page_ocr = items.iter().any(|i| i["font_name"].as_str() == Some("OCR"));
        ocr_used |= page_ocr;
        let confidence = page_ocr.then(|| {
            let values: Vec<f64> = items.iter().filter_map(|i| i["confidence"].as_f64()).collect();
            if values.is_empty() { 0.0 } else { values.iter().sum::<f64>() / values.len() as f64 }
        });
        let mut blocks: Vec<Block> = page["blocks"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .map(|b| Block { kind: b["kind"].as_str().unwrap_or("text").to_string(), text: block_text(b), bbox: corners(&b["bbox"]) })
                    .filter(|b| !b.text.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        if blocks.is_empty() {
            // No structure found: the lines themselves are the blocks.
            blocks = items
                .iter()
                .filter_map(|i| {
                    let text = i["text"].as_str()?.to_string();
                    let (x, y) = (number(i, "x"), number(i, "y"));
                    Some(Block { kind: "line".into(), text, bbox: Some([x, y, x + number(i, "width"), y + number(i, "height")]) })
                })
                .collect();
        }
        pages.push(Page {
            page: page["page"].as_u64().unwrap_or(0) as u32,
            width: number(&page, "width"),
            height: number(&page, "height"),
            text: page["text"].as_str().unwrap_or_default().to_string(),
            blocks,
            confidence,
        });
    }
    let total = document["total_pages"].as_u64().map_or(pages.len() as u32, |n| n as u32);
    let mut warnings = Vec::new();
    if (pages.len() as u32) < total {
        warnings.push(format!("{} of {total} pages were read", pages.len()));
    }
    OcrResult {
        text: pages.iter().map(|p| p.text.as_str()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n"),
        pages,
        total_pages: total,
        language: language.to_string(),
        engine: engine.to_string(),
        ocr_used,
        truncated: false,
        warnings,
    }
}

#[async_trait]
impl ActionBridge for LiteParse {
    async fn call(&self, action: &str, params: &Value) -> Result<Value, SendError> {
        match action {
            "parse" => self.parse(params).await,
            "status" => Ok(self.status().await),
            other => Err(SendError::permanent(format!("LiteParse has no action `{other}`"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recorded from `liteparse parse x.pdf --format json --extract-blocks --no-ocr` (a text layer).
    const TEXT_LAYER: &str = r#"{"total_pages":1,"pages":[{"page":1,"width":612.0,"height":792.0,
      "text":"Employment Contract\n\nSalary: 5000 per month",
      "text_items":[{"text":"Employment Contract","x":56.8,"y":56.7,"width":114.0,"height":11.3,"font_name":"LiberationMono","font_size":10.0,"confidence":1.0}],
      "blocks":[{"kind":"code","lines":["Employment Contract","Salary: 5000 per month"],"bbox":{"x":56.8,"y":56.7,"width":330.0,"height":56.4}}]}]}"#;

    /// Recorded from an image: blocks carry `text`, items are marked `OCR` with a confidence.
    const OCR: &str = r#"{"total_pages":1,"pages":[{"page":1,"width":612.0,"height":792.0,"text":"Employment Contract",
      "text_items":[{"text":"Employment","x":76.8,"y":77.7,"width":78.2,"height":12.4,"font_name":"OCR","font_size":12.4,"confidence":0.9},
                    {"text":"Contract","x":160.0,"y":77.7,"width":60.0,"height":12.4,"font_name":"OCR","font_size":12.4,"confidence":0.7}],
      "blocks":[{"kind":"heading","text":"Employment Contract","level":1,"bbox":{"x":76.8,"y":77.7,"width":143.2,"height":12.4}}]}]}"#;

    #[test]
    fn a_text_layer_page_becomes_blocks_with_corner_boxes() {
        let r = normalise(&serde_json::from_str(TEXT_LAYER).unwrap(), "eng", "liteparse");
        assert_eq!(r.total_pages, 1);
        assert!(!r.ocr_used);
        assert_eq!(r.pages[0].blocks[0].kind, "code");
        assert_eq!(r.pages[0].blocks[0].text, "Employment Contract\nSalary: 5000 per month");
        let b = r.pages[0].blocks[0].bbox.unwrap();
        assert!((b[2] - 386.8).abs() < 1e-6 && (b[3] - 113.1).abs() < 1e-6);
        assert!(r.pages[0].confidence.is_none());
    }

    #[test]
    fn an_ocr_page_says_so_and_averages_confidence() {
        let r = normalise(&serde_json::from_str(OCR).unwrap(), "eng", "liteparse");
        assert!(r.ocr_used);
        assert_eq!(r.pages[0].blocks[0].kind, "heading");
        assert!((r.pages[0].confidence.unwrap() - 0.8).abs() < 1e-9);
        assert_eq!(r.text, "Employment Contract");
    }

    #[test]
    fn a_page_without_blocks_falls_back_to_its_lines() {
        let doc = json!({"total_pages":1,"pages":[{"page":1,"width":1.0,"height":1.0,"text":"a","blocks":[],
            "text_items":[{"text":"a","x":1.0,"y":2.0,"width":3.0,"height":4.0,"font_name":"F","confidence":1.0}]}]});
        let r = normalise(&doc, "eng", "liteparse");
        assert_eq!(r.pages[0].blocks[0].kind, "line");
        assert_eq!(r.pages[0].blocks[0].bbox, Some([1.0, 2.0, 4.0, 6.0]));
    }

    #[test]
    fn unread_pages_are_a_warning() {
        let doc = json!({"total_pages":552,"pages":[{"page":1,"width":1.0,"height":1.0,"text":"","blocks":[],"text_items":[]}]});
        let r = normalise(&doc, "eng", "liteparse");
        assert_eq!(r.total_pages, 552);
        assert!(r.warnings[0].contains("1 of 552"));
    }

    #[tokio::test]
    async fn a_path_outside_the_work_folder_is_refused() {
        let bridge = LiteParse::new(&Values::new()).unwrap();
        let refused = bridge.call("parse", &json!({ "path": "/etc/passwd" })).await.unwrap_err();
        assert!(refused.permanent && refused.message.contains("prepared"));
    }

    #[tokio::test]
    async fn a_missing_program_is_reported_clearly() {
        let mut values = Values::new();
        values.insert("binary_path".into(), "/nonexistent/liteparse".into());
        let status = LiteParse::new(&values).unwrap().call("status", &json!({})).await.unwrap();
        assert_eq!(status["found"], false);
    }

    /// Needs the real program and sample files: `LITEPARSE_SAMPLES=<dir with sample.pdf, sample.png, sample.docx>
    /// cargo test -p liteparse -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn the_real_program_reads_a_pdf_an_image_and_a_word_file() {
        let dir = PathBuf::from(std::env::var("LITEPARSE_SAMPLES").expect("LITEPARSE_SAMPLES"));
        let bridge = LiteParse::new(&Values::new()).unwrap();
        let status = bridge.call("status", &json!({})).await.unwrap();
        assert_eq!(status["identity_ok"], true, "{status}");
        for (name, ocr) in [("sample.pdf", false), ("sample.png", true), ("sample.docx", false)] {
            let bytes = std::fs::read(dir.join(name)).unwrap();
            let ext = aether_ocr::extension_for(&bytes, Some(name)).unwrap();
            let input = aether_ocr::TempInput::create(&bytes, ext).unwrap();
            let got = bridge.call("parse", &json!({ "path": input.path(), "ocr": ocr })).await.unwrap();
            let text = got["text"].as_str().unwrap();
            assert!(text.contains("Employment Contract") && text.contains("5000"), "{name}: {text}");
            assert_eq!(got["engine"], "liteparse");
            if name == "sample.png" {
                assert_eq!(got["ocr_used"], true);
            }
        }
    }
}
