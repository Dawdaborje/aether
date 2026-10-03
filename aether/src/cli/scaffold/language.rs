//! Per-language plugin sources and build files.
//!
//! Aether runs plugins as WebAssembly modules through Extism, but generating a
//! plugin does not need the Extism CLI: each language only needs its own
//! toolchain (see the generated README).

use std::str::FromStr;

use super::ScaffoldError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Go,
    Rust,
    TypeScript,
    JavaScript,
    Python,
}

impl FromStr for Language {
    type Err = ScaffoldError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "go" | "golang" => Ok(Self::Go),
            "rust" | "rs" => Ok(Self::Rust),
            "typescript" | "ts" => Ok(Self::TypeScript),
            "javascript" | "js" => Ok(Self::JavaScript),
            "python" | "py" => Ok(Self::Python),
            _ => Err(ScaffoldError::UnsupportedLanguage(raw.to_string())),
        }
    }
}

/// A file to create, relative to the plugin directory.
pub struct TemplateFile {
    pub path: &'static str,
    pub content: String,
}

impl Language {
    pub fn label(self) -> &'static str {
        match self {
            Self::Go => "Go",
            Self::Rust => "Rust",
            Self::TypeScript => "TypeScript",
            Self::JavaScript => "JavaScript",
            Self::Python => "Python",
        }
    }

    /// Toolchain requirements and the command that produces `plugin.wasm`.
    fn build_notes(self) -> &'static str {
        match self {
            Self::Go => "Requires [TinyGo](https://tinygo.org) on your `PATH`.",
            Self::Rust => "Requires the `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`).",
            Self::TypeScript => "Requires `esbuild` and the [extism-js](https://github.com/extism/js-pdk) compiler on your `PATH`.",
            Self::JavaScript => "Requires the [extism-js](https://github.com/extism/js-pdk) compiler on your `PATH`.",
            Self::Python => "Requires the [extism-py](https://github.com/extism/python-pdk) compiler on your `PATH`.",
        }
    }

    /// Language-specific files: sources, `Makefile`, `.gitignore`. `make` (the
    /// default `build` target) writes `out/plugin.wasm`. Plugins deliberately
    /// declare no dependencies yet; the per-language SDKs will be added once
    /// they exist.
    pub fn files(self, name: &str) -> Vec<TemplateFile> {
        let render = |template: &str| template.replace("__NAME__", name);
        let file = |path, template: &str| TemplateFile {
            path,
            content: render(template),
        };
        let gitignore = |extra: &str| TemplateFile {
            path: ".gitignore",
            content: format!("out/\n{extra}"),
        };

        match self {
            Self::Go => vec![
                file("go.mod", GO_MOD),
                file("src/main.go", GO_MAIN),
                file("Makefile", GO_MAKEFILE),
                gitignore(""),
            ],
            Self::Rust => vec![
                file("Cargo.toml", RUST_CARGO),
                file("src/lib.rs", RUST_LIB),
                file("Makefile", RUST_MAKEFILE),
                gitignore("target/\n"),
            ],
            Self::TypeScript => vec![
                file("src/main.ts", TS_MAIN),
                file("src/main.d.ts", JS_INTERFACE),
                file("Makefile", TS_MAKEFILE),
                gitignore(""),
            ],
            Self::JavaScript => vec![
                file("src/main.js", JS_MAIN),
                file("src/main.d.ts", JS_INTERFACE),
                file("Makefile", JS_MAKEFILE),
                gitignore(""),
            ],
            Self::Python => vec![
                file("src/main.py", PY_MAIN),
                file("Makefile", PY_MAKEFILE),
                gitignore("__pycache__/\n"),
            ],
        }
    }

    pub fn readme(self, name: &str, label: &str) -> String {
        README
            .replace("__NAME__", name)
            .replace("__LABEL__", label)
            .replace("__LANGUAGE__", self.label())
            .replace("__BUILD__", self.build_notes())
    }
}

const README: &str = "# __LABEL__

An [Aether](https://github.com/Dawdaborje) plugin written in __LANGUAGE__.

## Build

__BUILD__

```sh
make        # builds out/plugin.wasm
make clean  # removes build output
```

`out/plugin.wasm` is the artifact named by `wasm_file` in `plugin.toml`.

This plugin has no dependencies yet. The sample export `hello` is a
placeholder; the Aether SDK for __LANGUAGE__ will be added once it is ready.

## Use

```sh
# register this package in the catalog (copies it into app_dir)
aether --app-dir <APP_DIR> --load-plugin .

# enable it for an organization
aether --install-plugin __NAME__ --org <ORG_DB>
```
";

const GO_MOD: &str = "module aether/plugins/__NAME__

go 1.24
";

const GO_MAIN: &str = r#"package main

// hello is a placeholder export. The Aether Go SDK will provide input and
// output helpers; until then it only reports success.
//
//go:wasmexport hello
func hello() int32 {
	return 0
}

// TinyGo requires a main function; it is never called.
func main() {}
"#;

const GO_MAKEFILE: &str = ".DEFAULT_GOAL := build

build:
\tmkdir -p out
\ttinygo build -target wasip1 -buildmode=c-shared -o out/plugin.wasm ./src

clean:
\trm -rf out

.PHONY: build clean
";

const RUST_CARGO: &str = r#"[package]
name = "__NAME__"
version = "0.1.0"
edition = "2021"
license = "MIT"

[lib]
crate-type = ["cdylib"]
"#;

const RUST_LIB: &str = r#"// `hello` is a placeholder export. The Aether Rust SDK will provide input and
// output helpers; until then it only reports success.
#[no_mangle]
pub extern "C" fn hello() -> i32 {
    0
}
"#;

const RUST_MAKEFILE: &str = ".DEFAULT_GOAL := build

build:
\tcargo build --release --target wasm32-unknown-unknown
\tmkdir -p out
\tcp target/wasm32-unknown-unknown/release/__NAME__.wasm out/plugin.wasm

clean:
\tcargo clean
\trm -rf out

.PHONY: build clean
";

const TS_MAIN: &str = r#"// `hello` is a placeholder export. The Aether TypeScript SDK will provide
// input and output helpers; until then it only reports success.
export function hello(): number {
  return 0;
}
"#;

const TS_MAKEFILE: &str = ".DEFAULT_GOAL := build

build:
\tmkdir -p out
\tesbuild src/main.ts --bundle --format=cjs --target=es2020 --outfile=out/main.js
\textism-js out/main.js -i src/main.d.ts -o out/plugin.wasm

clean:
\trm -rf out

.PHONY: build clean
";

/// Declares the exports for the extism-js compiler. The module name must be `main`.
const JS_INTERFACE: &str = "declare module 'main' {
  export function hello(): I32;
}
";

const JS_MAIN: &str = r#"// `hello` is a placeholder export. The Aether JavaScript SDK will provide
// input and output helpers; until then it only reports success.
function hello() {
  return 0;
}

module.exports = { hello };
"#;

const JS_MAKEFILE: &str = ".DEFAULT_GOAL := build

build:
\tmkdir -p out
\textism-js src/main.js -i src/main.d.ts -o out/plugin.wasm

clean:
\trm -rf out

.PHONY: build clean
";

const PY_MAIN: &str = r#"# The Aether Python SDK will provide the plugin entry points and input/output
# helpers. Until it exists this module only defines a placeholder.


def hello():
    return 0
"#;

const PY_MAKEFILE: &str = ".DEFAULT_GOAL := build

build:
\tmkdir -p out
\textism-py src/main.py -o out/plugin.wasm

clean:
\trm -rf out

.PHONY: build clean
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_language_names_and_aliases() -> Result<(), ScaffoldError> {
        for (raw, expected) in [
            ("go", Language::Go),
            ("Rust", Language::Rust),
            ("ts", Language::TypeScript),
            ("JavaScript", Language::JavaScript),
            ("py", Language::Python),
        ] {
            assert_eq!(raw.parse::<Language>()?, expected);
        }
        assert!("cobol".parse::<Language>().is_err());
        Ok(())
    }

    #[test]
    fn every_language_has_sources_a_makefile_and_no_unfilled_placeholders() {
        for language in [
            Language::Go,
            Language::Rust,
            Language::TypeScript,
            Language::JavaScript,
            Language::Python,
        ] {
            let files = language.files("company");
            assert!(files.iter().any(|f| f.path.starts_with("src/")));
            assert!(files.iter().any(|f| f.path == "Makefile"));
            assert!(
                files.iter().all(|f| !f.content.contains("__NAME__")),
                "{language:?} left a placeholder"
            );
            assert!(!language.readme("company", "Company").contains("__"));
        }
    }

    #[test]
    fn makefiles_default_to_building_into_out_with_tab_recipes() {
        for language in [
            Language::Go,
            Language::Rust,
            Language::TypeScript,
            Language::JavaScript,
            Language::Python,
        ] {
            let files = language.files("company");
            let makefile = files.iter().find(|f| f.path == "Makefile");
            let Some(makefile) = makefile else {
                panic!("{language:?} has no Makefile");
            };
            assert!(makefile.content.starts_with(".DEFAULT_GOAL := build"));
            assert!(makefile.content.contains("out/plugin.wasm"), "{language:?}");
            assert!(makefile.content.contains("\n\tmkdir -p out"), "{language:?}");
            let ignore = files.iter().find(|f| f.path == ".gitignore");
            assert!(ignore.is_some_and(|f| f.content.starts_with("out/")));
        }
    }

    #[test]
    fn generated_plugins_declare_no_dependencies() {
        let paths = |language: Language| -> Vec<&'static str> {
            language.files("company").iter().map(|f| f.path).collect()
        };
        let content = |language: Language, path: &str| {
            language
                .files("company")
                .into_iter()
                .find(|f| f.path == path)
                .map(|f| f.content)
                .unwrap_or_default()
        };

        assert!(!content(Language::Rust, "Cargo.toml").contains("[dependencies]"));
        assert!(!content(Language::Go, "go.mod").contains("require"));
        for language in [Language::TypeScript, Language::JavaScript, Language::Python] {
            let listed = paths(language);
            assert!(!listed.contains(&"package.json"), "{language:?}");
            assert!(!listed.contains(&"tsconfig.json"), "{language:?}");
        }
        for language in [Language::Go, Language::Rust, Language::TypeScript, Language::JavaScript, Language::Python] {
            for file in language.files("company") {
                assert!(!file.content.contains("extism_pdk") && !file.content.contains("go-pdk") && !file.content.contains("@extism/js-pdk"), "{language:?} {}", file.path);
            }
        }
    }
}
