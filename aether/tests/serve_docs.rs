//! Runs the real `aether --serve-docs` with plugin folders and talks HTTP to it.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn plugin(root: &Path, name: &str, docs_table: &str, readme: &str) {
    let dir = root.join(name);
    std::fs::create_dir_all(dir.join("docs")).unwrap();
    std::fs::write(
        dir.join("plugin.toml"),
        format!("[plugin]\nname = \"{name}\"\nlabel = \"{name} tool\"\nversion = \"1.0.0\"\nscript = \"main.rhai\"\ncapabilities = []\n{docs_table}"),
    )
    .unwrap();
    std::fs::write(dir.join("main.rhai"), "fn ping() { 1 }\n").unwrap();
    std::fs::write(dir.join("docs/README.md"), readme).unwrap();
}

struct Server {
    child: Child,
    port: u16,
    log: std::path::PathBuf,
}

impl Server {
    fn start(root: &Path, extra: &[&str]) -> Server {
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let log = root.join("server.log");
        let child = Command::new(env!("CARGO_BIN_EXE_aether"))
            .args(["--serve-docs", &format!("127.0.0.1:{port}"), "--plugin-docs"])
            .arg(root.join("notes"))
            .arg("--plugin-docs")
            .arg(root.join("secretive"))
            .args(extra)
            .current_dir(root) // no aether.toml here
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(&log).unwrap())
            .spawn()
            .unwrap();
        let server = Server { child, port, log };
        let start = Instant::now();
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(start.elapsed() < Duration::from_secs(20), "the server did not start: {}", server.log_text());
            std::thread::sleep(Duration::from_millis(100));
        }
        server
    }

    fn get(&self, path: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).unwrap();
        let status = raw.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        (status, raw)
    }

    fn log_text(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn two_plugins() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    plugin(dir.path(), "notes", "\n[docs]\nvisibility = \"public\"\n", "# Notes\n\nTake notes with the notes plugin.\n");
    plugin(dir.path(), "secretive", "\n[docs]\n", "# Secretive\n\nInternal details nobody outside should read.\n");
    dir
}

#[test]
fn shows_public_plugin_docs_and_says_why_others_are_hidden() {
    let dir = two_plugins();
    let server = Server::start(dir.path(), &[]);

    let (status, body) = server.get("/plugins/notes");
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("Take notes with the notes plugin"));
    let (_, home) = server.get("/");
    assert!(home.contains("notes tool") && !home.contains("secretive"), "the hidden plugin must not appear anywhere");

    // The default visibility is `authenticated`, and this server has no sign-in: a plain 404, same as a missing page.
    let (status, body) = server.get("/plugins/secretive");
    assert_eq!(status, 404);
    assert!(!body.contains("Internal details") && !body.contains("secretive"));
    let (_, found) = server.get("/search?q=internal+details");
    assert!(!found.contains("Internal details"));

    // The author is told in the log why it is missing.
    let log = server.log_text();
    assert!(log.contains("secretive (authenticated)") && log.contains("--docs-all"), "{log}");
}

#[test]
fn docs_all_previews_everything_on_this_machine_only() {
    let dir = two_plugins();
    let server = Server::start(dir.path(), &["--docs-all"]);
    let (status, body) = server.get("/plugins/secretive");
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("Internal details nobody outside should read"));
    drop(server);

    // On an address other machines can reach, the same flag is refused.
    let output = Command::new(env!("CARGO_BIN_EXE_aether"))
        .args(["--serve-docs", "0.0.0.0:0", "--docs-all", "--plugin-docs"])
        .arg(dir.path().join("secretive"))
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("loopback"), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn an_edited_page_shows_after_a_moment_and_broken_docs_are_skipped_with_a_reason() {
    let dir = two_plugins();
    let server = Server::start(dir.path(), &["--docs-all"]);
    assert!(server.get("/plugins/notes").1.contains("Take notes"));

    std::fs::write(dir.path().join("notes/docs/README.md"), "# Notes\n\nA freshly edited sentence.\n").unwrap();
    std::fs::write(dir.path().join("secretive/docs/README.md"), "[nowhere](missing.md)").unwrap();
    let start = Instant::now();
    loop {
        let (_, body) = server.get("/plugins/notes");
        if body.contains("freshly edited sentence") {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(20), "the edit never appeared");
        std::thread::sleep(Duration::from_millis(500));
    }
    assert_eq!(server.get("/plugins/secretive").0, 404, "docs with problems are not shown");
    assert!(server.log_text().contains("missing.md"), "{}", server.log_text());
}

#[test]
fn exports_the_site_as_static_files_under_a_base_path() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("site");
    let output = Command::new(env!("CARGO_BIN_EXE_aether"))
        .args(["--export-docs"])
        .arg(&out)
        .args(["--docs-base", "/aether-docs"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Wrote "), "{}", String::from_utf8_lossy(&output.stdout));
    let home = std::fs::read_to_string(out.join("index.html")).unwrap();
    assert!(home.contains("href=\"/aether-docs/assets/site.css\""), "{home}");
    assert!(out.join("plugin/pages/index.html").is_file() && out.join("llms.txt").is_file() && out.join(".nojekyll").exists());

    // --docs-base only makes sense with --export-docs.
    let refused = Command::new(env!("CARGO_BIN_EXE_aether")).args(["--docs-base", "/x"]).output().unwrap();
    assert!(!refused.status.success());
}
