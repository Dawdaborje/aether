//! Process logging: stderr and/or a file under `app_dir` / CLI overrides.

use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    str::FromStr,
};

use aether_core::config_manager::services::peek_logging;
use log::LevelFilter;

use super::args::Args;

/// Where log lines go after `init_logger`.
#[derive(Debug, Clone)]
pub struct LogDestination {
    /// Append target; `None` means stderr only.
    pub file: Option<PathBuf>,
    /// Also write to stderr when `file` is set. Ignored when `file` is `None`.
    pub stdout: bool,
}

/// Resolve file path + stdout from CLI flags and an optional `aether.toml` peek.
pub fn resolve_log_destination(args: &Args) -> LogDestination {
    let want_stdout = !args.no_log_stdout;
    if args.no_log_file {
        return LogDestination {
            file: None,
            stdout: true,
        };
    }
    if let Some(path) = args.log_file.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        return LogDestination {
            file: Some(absolute_from_cwd(path)),
            stdout: want_stdout,
        };
    }

    let app_dir_override = args.app_dir.as_deref().map(absolute_from_cwd);
    let (mut logging, app_dir) = peek_logging(peek_config_file(args).as_deref(), app_dir_override);
    if let Some(dir) = args.log_dir.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        logging.file = true;
        // Resolve against cwd so relative `--log-dir` is not tied to `app_dir`.
        logging.dir = absolute_from_cwd(dir).to_string_lossy().into_owned();
    }

    let file = if logging.file {
        let base = app_dir.unwrap_or_else(|| {
            env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        });
        logging.file_path(&base)
    } else {
        None
    };

    let stdout = if file.is_some() {
        logging.stdout && want_stdout
    } else {
        true
    };
    LogDestination { file, stdout }
}

fn absolute_from_cwd(raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        return path;
    }
    env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| PathBuf::from(raw))
}

fn peek_config_file(args: &Args) -> Option<PathBuf> {
    if let Some(path) = args.config_file.as_deref() {
        let path = PathBuf::from(path);
        return path.exists().then_some(path);
    }
    let default = PathBuf::from("aether.toml");
    default.exists().then_some(default)
}

struct Tee<A: Write, B: Write> {
    a: A,
    b: B,
}

impl<A: Write, B: Write> Write for Tee<A, B> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.a.write(buf)?;
        let _ = self.b.write_all(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.a.flush()?;
        self.b.flush()?;
        Ok(())
    }
}

/// Configure `env_logger` for stderr and/or a log file.
pub fn init_logger(args: &Args) {
    let level = match LevelFilter::from_str(&args.log) {
        Ok(l) => l,
        Err(_) => {
            eprintln!("Invalid log level '{}', defaulting to 'debug'", args.log);
            LevelFilter::Debug
        }
    };

    let destination = resolve_log_destination(args);
    let mut builder = env_logger::Builder::new();
    builder.filter_level(level);

    let enable_framework_debug =
        args.verbose || matches!(level, LevelFilter::Debug | LevelFilter::Trace);
    if enable_framework_debug {
        builder
            .filter_module("axum", LevelFilter::Debug)
            .filter_module("tower_http", LevelFilter::Debug)
            .filter_module("hyper", LevelFilter::Debug);
    }

    // The WASM compiler logs a line per pass per function at debug level: thousands of
    // lines for one plugin, written while the compile runs. Keep its warnings only, even
    // when the rest of the program is at debug.
    builder
        .filter_module("cranelift_codegen", LevelFilter::Warn)
        .filter_module("cranelift_frontend", LevelFilter::Warn)
        .filter_module("cranelift_entity", LevelFilter::Warn)
        .filter_module("wasmtime", LevelFilter::Warn)
        .filter_module("wasmtime_internal_cranelift", LevelFilter::Warn)
        .filter_module("wasmtime_internal_cache", LevelFilter::Warn)
        .filter_module("regalloc2", LevelFilter::Warn)
        .filter_module("extism", LevelFilter::Info);

    if let Some(path) = &destination.file {
        if let Some(parent) = path.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            eprintln!(
                "Could not create log directory {}: {error}; logging to stderr only",
                parent.display()
            );
            builder.target(env_logger::Target::Stderr).init();
            return;
        }
        match OpenOptions::new().create(true).append(true).open(path) {
            Ok(file) => {
                let writer: Box<dyn Write + Send> = if destination.stdout {
                    Box::new(Tee {
                        a: file,
                        b: io::stderr(),
                    })
                } else {
                    Box::new(file)
                };
                builder.target(env_logger::Target::Pipe(writer));
                eprintln!("Logging to {}", path.display());
            }
            Err(error) => {
                eprintln!(
                    "Could not open log file {}: {error}; logging to stderr only",
                    path.display()
                );
                builder.target(env_logger::Target::Stderr);
            }
        }
    } else {
        builder.target(env_logger::Target::Stderr);
    }

    builder.init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use clap::Parser;

    fn args_with(rest: &[&str]) -> Args {
        Args::try_parse_from(std::iter::once("aether").chain(rest.iter().copied()))
            .expect("parse args")
    }

    #[test]
    fn no_log_file_disables_file() {
        let dest = resolve_log_destination(&args_with(&[
            "--no-log-file",
            "--log-file",
            "/tmp/x.log",
        ]));
        assert!(dest.file.is_none());
    }

    #[test]
    fn log_file_flag_sets_path() {
        let dest = resolve_log_destination(&args_with(&["--log-file", "/tmp/aether-test.log"]));
        assert_eq!(
            dest.file.as_deref(),
            Some(Path::new("/tmp/aether-test.log"))
        );
        assert!(dest.stdout);
    }

    #[test]
    fn no_log_stdout_with_file() {
        let dest = resolve_log_destination(&args_with(&[
            "--log-file",
            "/tmp/aether-test.log",
            "--no-log-stdout",
        ]));
        assert!(!dest.stdout);
    }

    #[test]
    fn log_dir_enables_file_under_dir() {
        let dest = resolve_log_destination(&args_with(&["--log-dir", "/var/log/aether"]));
        assert_eq!(
            dest.file.as_deref(),
            Some(Path::new("/var/log/aether/aether.log"))
        );
        assert!(dest.stdout);
    }
}
