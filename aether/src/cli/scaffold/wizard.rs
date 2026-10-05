//! `aether --gen` with nothing after it: asks what to create and how, then returns the plan.
//!
//! The questions are asked on any reader and writer, so the whole conversation is tested with scripted
//! answers. Pressing Enter takes the default shown in brackets. The wizard only collects answers; the
//! caller creates the files through the same functions the command-line options use, so a wizard run
//! and `aether --gen plugin --plugin-path …` always produce the same thing.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use super::{Language, PluginDetails, WorkspaceDetails, humanize, validate_name};

/// How many times a bad answer is asked again before giving up.
const ATTEMPTS: usize = 5;

#[derive(Debug, thiserror::Error)]
pub enum WizardError {
    #[error("the input ended before the questions were answered; run `aether --gen plugin --plugin-path <path> --plugin-language <language>` to create a plugin without questions")]
    InputEnded,
    #[error("no valid answer to `{0}`")]
    TooManyAttempts(String),
    #[error("cancelled; nothing was created")]
    Cancelled,
    #[error("could not talk to the terminal: {0}")]
    Io(#[from] std::io::Error),
}

/// What the person asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    Plugin { path: PathBuf, language: Language, details: PluginDetails },
    Workspace { path: PathBuf, details: WorkspaceDetails },
    Config { path: PathBuf },
}

/// What the wizard knows about where it is running, to suggest defaults.
#[derive(Debug, Clone, Default)]
pub struct Surroundings {
    /// The workspace the current folder is in, with its first author.
    pub workspace: Option<(String, Option<(String, String)>)>,
}

struct Prompter<R, W> {
    input: R,
    out: W,
}

impl<R: BufRead, W: Write> Prompter<R, W> {
    fn line(&mut self) -> Result<String, WizardError> {
        let mut text = String::new();
        if self.input.read_line(&mut text)? == 0 {
            return Err(WizardError::InputEnded);
        }
        Ok(text.trim().to_string())
    }

    fn say(&mut self, text: &str) -> Result<(), WizardError> {
        writeln!(self.out, "{text}")?;
        Ok(())
    }

    /// A free answer; Enter takes the default (there is none when `default` is empty and an answer is required).
    fn ask(&mut self, question: &str, default: &str) -> Result<String, WizardError> {
        for _ in 0..ATTEMPTS {
            if default.is_empty() {
                write!(self.out, "{question}: ")?;
            } else {
                write!(self.out, "{question} [{default}]: ")?;
            }
            self.out.flush()?;
            let answer = self.line()?;
            if !answer.is_empty() {
                return Ok(answer);
            }
            if !default.is_empty() {
                return Ok(default.to_string());
            }
        }
        Err(WizardError::TooManyAttempts(question.to_string()))
    }

    /// An answer that must pass `check`, which says what is wrong when it does not.
    fn ask_checked(&mut self, question: &str, default: &str, check: impl Fn(&str) -> Result<(), String>) -> Result<String, WizardError> {
        for _ in 0..ATTEMPTS {
            let answer = self.ask(question, default)?;
            match check(&answer) {
                Ok(()) => return Ok(answer),
                Err(problem) => self.say(&format!("  {problem}"))?,
            }
        }
        Err(WizardError::TooManyAttempts(question.to_string()))
    }

    /// One of a numbered list, by number or by name (or the start of it); Enter takes the first.
    fn choose<T: Clone>(&mut self, question: &str, options: &[(&str, &str, T)]) -> Result<T, WizardError> {
        self.say(question)?;
        for (index, (key, label, _)) in options.iter().enumerate() {
            self.say(&format!("  {}) {key:<12} {label}", index + 1))?;
        }
        for _ in 0..ATTEMPTS {
            write!(self.out, "Choose [1]: ")?;
            self.out.flush()?;
            let answer = self.line()?.to_ascii_lowercase();
            if answer.is_empty() {
                return Ok(options[0].2.clone());
            }
            if let Some(index) = answer.parse::<usize>().ok().and_then(|n| n.checked_sub(1)).filter(|i| *i < options.len()) {
                return Ok(options[index].2.clone());
            }
            let matching: Vec<_> = options.iter().filter(|(key, _, _)| key.starts_with(&answer)).collect();
            if let [only] = matching.as_slice() {
                return Ok(only.2.clone());
            }
            self.say(&format!("  Type a number from 1 to {}, or a name from the list.", options.len()))?;
        }
        Err(WizardError::TooManyAttempts(question.to_string()))
    }

    fn confirm(&mut self, question: &str) -> Result<bool, WizardError> {
        for _ in 0..ATTEMPTS {
            write!(self.out, "{question} [Y/n]: ")?;
            self.out.flush()?;
            match self.line()?.to_ascii_lowercase().as_str() {
                "" | "y" | "yes" => return Ok(true),
                "n" | "no" => return Ok(false),
                _ => self.say("  Answer y or n.")?,
            }
        }
        Err(WizardError::TooManyAttempts(question.to_string()))
    }

    fn author(&mut self, default: Option<&(String, String)>) -> Result<(String, String), WizardError> {
        let name = self.ask("Author name", default.map_or("Your Name", |(name, _)| name.as_str()))?;
        let email = self.ask("Author email", default.map_or("your.email@example.com", |(_, email)| email.as_str()))?;
        Ok((name, email))
    }
}

/// The last part of `path` is the name of what is created.
fn last_segment(path: &str) -> &str {
    Path::new(path).file_name().and_then(|name| name.to_str()).unwrap_or("")
}

/// A place nothing is in yet: absent, or an empty folder.
fn free_place(path: &str, base: &Path) -> Result<(), String> {
    let full = base.join(path);
    match std::fs::read_dir(&full) {
        Ok(mut entries) => entries.next().map_or(Ok(()), |_| Err(format!("{} already exists and is not empty", full.display()))),
        Err(_) if full.exists() => Err(format!("{} exists and is not a folder", full.display())),
        Err(_) => Ok(()),
    }
}

/// Ask what to create and how. `base` is where relative paths will be created (the current folder).
pub fn run<R: BufRead, W: Write>(input: R, out: W, base: &Path, around: &Surroundings) -> Result<Plan, WizardError> {
    let mut p = Prompter { input, out };
    p.say("Aether: create something new. Press Enter to take the answer in [brackets].\n")?;
    let what = p.choose(
        "What do you want to create?",
        &[
            ("plugin", "code and data that extend Aether", 0),
            ("workspace", "a folder that groups related plugins", 1),
            ("config", "an aether.toml configuration file", 2),
        ],
    )?;
    p.say("")?;

    let plan = match what {
        0 => {
            let language = p.choose(
                "Which language?",
                &[
                    ("rhai", "a script; nothing to compile (best for small plugins)", Language::Rhai),
                    ("rust", "WebAssembly module", Language::Rust),
                    ("go", "WebAssembly module (TinyGo)", Language::Go),
                    ("typescript", "WebAssembly module", Language::TypeScript),
                    ("javascript", "WebAssembly module", Language::JavaScript),
                    ("python", "WebAssembly module", Language::Python),
                ],
            )?;
            p.say("")?;
            if let Some((name, _)) = &around.workspace {
                p.say(&format!("You are inside the workspace '{name}': a plugin created in it is registered there."))?;
            }
            let path = p.ask_checked("Where? (the last part is the plugin's name)", "my_plugin", |answer| {
                validate_name(last_segment(answer)).map_err(|e| e.to_string())?;
                free_place(answer, base)
            })?;
            let name = last_segment(&path).to_string();
            let label = p.ask("Label (shown to people)", &humanize(&name))?;
            let description = p.ask("Description", &format!("A short description of {label}."))?;
            let default_author = around.workspace.as_ref().and_then(|(_, author)| author.as_ref());
            let author = p.author(default_author)?;
            p.say(&format!("\nA {} plugin '{name}' at {}.", language.label(), base.join(&path).display()))?;
            if !p.confirm("Create it?")? {
                return Err(WizardError::Cancelled);
            }
            Plan::Plugin {
                path: PathBuf::from(path),
                language,
                details: PluginDetails { label: Some(label), description: Some(description), author: Some(author) },
            }
        }
        1 => {
            let path = p.ask_checked("Where? (the last part is the folder's name)", "my_workspace", |answer| {
                validate_name(last_segment(answer)).map_err(|e| e.to_string())?;
                free_place(answer, base)
            })?;
            let name = p.ask_checked("Workspace name", last_segment(&path), |answer| validate_name(answer).map_err(|e| e.to_string()))?;
            let label = p.ask("Label (shown to people)", &humanize(&name))?;
            let description = p.ask("Description", &format!("The plugins of {label}."))?;
            let author = p.author(None)?;
            p.say(&format!("\nA workspace '{name}' at {}.", base.join(&path).display()))?;
            if !p.confirm("Create it?")? {
                return Err(WizardError::Cancelled);
            }
            Plan::Workspace {
                path: PathBuf::from(path),
                details: WorkspaceDetails { name: Some(name), label: Some(label), description: Some(description), author: Some(author) },
            }
        }
        _ => {
            let path = p.ask_checked("Where to write it?", "aether.toml", |answer| {
                if base.join(answer).is_dir() {
                    Err("that is a folder".to_string())
                } else if base.join(answer).exists() {
                    Err("that file already exists; choose another name".to_string())
                } else {
                    Ok(())
                }
            })?;
            if !p.confirm(&format!("Write {}?", base.join(&path).display()))? {
                return Err(WizardError::Cancelled);
            }
            Plan::Config { path: PathBuf::from(path) }
        }
    };
    p.say("")?;
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn answer(script: &str, base: &Path, around: &Surroundings) -> (Result<Plan, WizardError>, String) {
        let mut out = Vec::new();
        let result = run(Cursor::new(script.to_string()), &mut out, base, around);
        (result, String::from_utf8_lossy(&out).into_owned())
    }

    #[test]
    fn enter_everywhere_makes_a_rhai_plugin_with_the_defaults() {
        let dir = tempfile::tempdir().map(|d| d.keep()).unwrap_or_default();
        let (plan, shown) = answer("\n\n\n\n\n\n\n\n", &dir, &Surroundings::default());
        assert_eq!(
            plan.ok(),
            Some(Plan::Plugin {
                path: PathBuf::from("my_plugin"),
                language: Language::Rhai,
                details: PluginDetails {
                    label: Some("My Plugin".into()),
                    description: Some("A short description of My Plugin.".into()),
                    author: Some(("Your Name".into(), "your.email@example.com".into())),
                },
            })
        );
        assert!(shown.contains("What do you want to create?") && shown.contains("Which language?"), "{shown}");
        assert!(shown.contains("rhai") && shown.contains("nothing to compile"), "{shown}");
    }

    #[test]
    fn answers_by_number_or_name_and_every_detail_can_be_chosen() {
        let dir = tempfile::tempdir().map(|d| d.keep()).unwrap_or_default();
        // plugin (by name), rust (by number), a nested path, label, description, author, confirm.
        let (plan, _) = answer("plug\n2\nbase/currency\nCurrencies\nRates and conversion\nAda\nada@x.io\ny\n", &dir, &Surroundings::default());
        assert_eq!(
            plan.ok(),
            Some(Plan::Plugin {
                path: PathBuf::from("base/currency"),
                language: Language::Rust,
                details: PluginDetails {
                    label: Some("Currencies".into()),
                    description: Some("Rates and conversion".into()),
                    author: Some(("Ada".into(), "ada@x.io".into())),
                },
            })
        );
    }

    #[test]
    fn a_workspace_has_a_name_and_the_author_is_asked_once() {
        let dir = tempfile::tempdir().map(|d| d.keep()).unwrap_or_default();
        let (plan, _) = answer("2\nstuff/base\nbase\nBase\nThe foundation\nAda\nada@x.io\nyes\n", &dir, &Surroundings::default());
        assert_eq!(
            plan.ok(),
            Some(Plan::Workspace {
                path: PathBuf::from("stuff/base"),
                details: WorkspaceDetails {
                    name: Some("base".into()),
                    label: Some("Base".into()),
                    description: Some("The foundation".into()),
                    author: Some(("Ada".into(), "ada@x.io".into())),
                },
            })
        );
    }

    #[test]
    fn a_config_file_is_never_written_over_another() {
        let dir = tempfile::tempdir().map(|d| d.keep()).unwrap_or_default();
        std::fs::write(dir.join("aether.toml"), "x").ok();
        let (plan, shown) = answer("3\naether.toml\nother.toml\ny\n", &dir, &Surroundings::default());
        assert_eq!(plan.ok(), Some(Plan::Config { path: PathBuf::from("other.toml") }));
        assert!(shown.contains("already exists"), "{shown}");
    }

    #[test]
    fn bad_answers_are_explained_and_asked_again() {
        let dir = tempfile::tempdir().map(|d| d.keep()).unwrap_or_default();
        std::fs::create_dir_all(dir.join("taken")).ok();
        std::fs::write(dir.join("taken/file"), "x").ok();
        // a bad choice, a bad name, a name that is taken, then a good one
        let (plan, shown) = answer("what\n1\n9\nrust\nBad-Name\ntaken\nfresh_one\n\n\n\n\ny\n", &dir, &Surroundings::default());
        assert!(matches!(plan, Ok(Plan::Plugin { language: Language::Rust, .. })), "{plan:?}");
        assert!(shown.contains("Type a number from 1 to 3"), "{shown}");
        assert!(shown.contains("Type a number from 1 to 6"), "{shown}");
        assert!(shown.contains("invalid plugin name `Bad-Name`"), "{shown}");
        assert!(shown.contains("already exists and is not empty"), "{shown}");
    }

    #[test]
    fn the_workspace_around_is_mentioned_and_lends_its_author() {
        let dir = tempfile::tempdir().map(|d| d.keep()).unwrap_or_default();
        let around = Surroundings { workspace: Some(("base".into(), Some(("Ada".into(), "ada@x.io".into())))) };
        let (plan, shown) = answer("\n\ncurrency\n\n\n\n\n\n", &dir, &around);
        assert!(shown.contains("inside the workspace 'base'"), "{shown}");
        assert!(matches!(&plan, Ok(Plan::Plugin { details, .. }) if details.author == Some(("Ada".into(), "ada@x.io".into()))), "{plan:?}");
    }

    #[test]
    fn saying_no_creates_nothing_and_a_closed_input_is_reported() {
        let dir = tempfile::tempdir().map(|d| d.keep()).unwrap_or_default();
        let (plan, _) = answer("\n\n\n\n\n\n\nn\n", &dir, &Surroundings::default());
        assert!(matches!(plan, Err(WizardError::Cancelled)));
        let (plan, _) = answer("1\n", &dir, &Surroundings::default());
        assert!(matches!(plan, Err(WizardError::InputEnded)));
        let (plan, _) = answer("x\nx\nx\nx\nx\n", &dir, &Surroundings::default());
        assert!(matches!(plan, Err(WizardError::TooManyAttempts(_))));
    }
}
