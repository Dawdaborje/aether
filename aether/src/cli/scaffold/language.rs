//! Per-language plugin sources and build files.
//!
//! Aether runs plugins as WebAssembly modules through Extism, but generating a
//! plugin does not need the Extism CLI: each language only needs its own
//! toolchain (see the generated README). Rhai and Lua are the exceptions: such a
//! plugin is its script, with nothing to compile.

use std::str::FromStr;

use super::ScaffoldError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Go,
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Rhai,
    Lua,
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
            "rhai" => Ok(Self::Rhai),
            "lua" | "luau" => Ok(Self::Lua),
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
            Self::Rhai => "Rhai",
            Self::Lua => "Lua",
        }
    }

    /// Whether the plugin is compiled to a WebAssembly module (every language but the scripts).
    pub fn is_wasm(self) -> bool {
        !matches!(self, Self::Rhai | Self::Lua)
    }

    /// The script file of a script plugin.
    pub fn script_file(self) -> Option<&'static str> {
        match self {
            Self::Rhai => Some("main.rhai"),
            Self::Lua => Some("main.lua"),
            _ => None,
        }
    }

    /// The line of `plugin.toml` that names the plugin's code.
    pub fn code_line(self) -> &'static str {
        match self {
            Self::Rhai => "script = \"main.rhai\"",
            Self::Lua => "script = \"main.lua\"",
            _ => "wasm_file = \"out/plugin.wasm\"",
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
            Self::Rhai | Self::Lua => "Nothing to install: the script is read by Aether itself.",
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
            Self::Rhai => vec![
                file("main.rhai", RHAI_MAIN),
                // Nothing is built, so only editor leftovers are ignored.
                TemplateFile { path: ".gitignore", content: SCRIPT_GITIGNORE.to_string() },
            ],
            Self::Lua => vec![
                file("main.lua", LUA_MAIN),
                TemplateFile { path: ".gitignore", content: SCRIPT_GITIGNORE.to_string() },
            ],
        }
    }

    pub fn readme(self, name: &str, label: &str) -> String {
        match self {
            Self::Rhai => return RHAI_README.replace("__NAME__", name).replace("__LABEL__", label),
            Self::Lua => return LUA_README.replace("__NAME__", name).replace("__LABEL__", label),
            _ => {}
        }
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
license = "BUSL-1.1"

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


const SCRIPT_GITIGNORE: &str = "*~\n*.swp\n.DS_Store\n";

const LUA_MAIN: &str = r#"-- __LABEL__, an Aether plugin written in Lua (Luau). There is nothing to compile: this file is the plugin.
--
-- Every global function can be called (POST /api/plugins/__NAME__/<function>, from a page, or with
-- `aether --command` once it is declared in plugin.toml). It takes the call's JSON input as its one
-- parameter and returns JSON. A `local function`, or one whose name starts with `_`, is a helper
-- nobody can call. The top level may only define functions and constants. The kernel commands
-- (db, cache, fs, communication, scheduler, plugins, ...) are listed in docs/plugin/commands.md, and
-- the capabilities each one needs go in `capabilities` in plugin.toml. JSON null is nil.

function hello(input)
    return { message = "Hello from __NAME__", actor = context.get().actor }
end

-- A function that reports a mistake to the caller:
--
-- function greet(input)
--     if input.name == nil or input.name == "" then fail("a name is required") end
--     return "Hello " .. input.name
-- end
--
-- Another plugin's function, in any language (list it under `dependencies` in plugin.toml):
--
-- local total = plugins.call("billing", "total", { order = input.order })
"#;

const LUA_README: &str = r#"# __LABEL__

An [Aether](https://github.com/Dawdaborje) plugin written in Lua ([Luau](https://luau.org)): the plugin is the
script `main.lua`, with nothing to compile.

## Files

| File | |
|---|---|
| `plugin.toml` | name, version, `capabilities`, `access_models`, `dependencies`, pages' app tile, commands, schedules |
| `main.lua` | the functions; every global function is callable |
| `models/<name>.json` | the plugin's data models (optional); `aether --sync-models .` gives them their ids |
| `pages/*.xml` | the pages (optional) |

## Work on it

```sh
aether --sync-models .                  # give the models ids
aether --load-plugin .                  # register the plugin in the catalog
aether --install-plugin __NAME__ --org <org>   # install it for an organization
aether --upgrade-plugin __NAME__ --org <org>   # move an organization to the version you just loaded
```

Loading reads the script and refuses one that does not compile. Add `-c path/to/aether.toml` when the configuration
is not in the usual place. While developing, `aether --serve --watch` loads and upgrades by itself whenever a file
changes.

A plugin that links to another plugin's model (`"target": "currency.currency"`) needs that plugin loaded first;
list it under `dependencies`. The same list lets `plugins.call(plugin, function, input)` reach that plugin's
functions, whether it is written in Lua, Rhai or compiled to WebAssembly.

## Learn more

* `docs/plugin/lua.md`: the language as Aether runs it, its limits and the commands a script can call.
* `docs/plugin/commands.md`: every kernel command, its capability and payload.
* `docs/plugin/pages.md` and `docs/plugin/cli-commands.md`: pages, and commands run with `aether --command`.
"#;

const RHAI_MAIN: &str = r#"// __LABEL__, an Aether plugin written in Rhai. There is nothing to compile: this file is the plugin.
//
// Every public `fn` can be called (POST /api/plugins/__NAME__/<function>, from a page, or with
// `aether --command` once it is declared in plugin.toml). It takes the call's JSON input as its one
// parameter and returns JSON. `private fn` is for helpers. Rhai passes maps and arrays by value, so a
// helper that changes one must return it. The kernel commands (db::, cache::, fs::, communication::,
// scheduler::, ...) are listed in docs/plugin/commands.md, and the capabilities each one needs go in
// `capabilities` in plugin.toml.

fn hello(input) {
    #{ message: "Hello from __NAME__", actor: context::get().actor }
}

// A function that reports a mistake to the caller:
//
// fn greet(input) {
//     if input.name == () || input.name == "" { fail("a name is required"); }
//     "Hello " + input.name
// }
"#;

const RHAI_README: &str = r#"# __LABEL__

An [Aether](https://github.com/Dawdaborje) plugin written in Rhai: the plugin is the script `main.rhai`,
with nothing to compile.

## Files

| File | |
|---|---|
| `plugin.toml` | name, version, `capabilities`, `access_models`, `dependencies`, pages' app tile, commands, schedules |
| `main.rhai` | the functions; every public `fn` is callable |
| `models/<name>.json` | the plugin's data models (optional); `aether --sync-models .` gives them their ids |
| `pages/*.xml` | the pages (optional) |

## Work on it

```sh
aether --sync-models .                  # give the models ids
aether --load-plugin .                  # register the plugin in the catalog
aether --install-plugin __NAME__ --org <org>   # install it for an organization
aether --upgrade-plugin __NAME__ --org <org>   # move an organization to the version you just loaded
```

Loading reads the script and refuses one that does not compile. Add `-c path/to/aether.toml` when the configuration
is not in the usual place. While developing, `aether --serve --watch` loads and upgrades by itself whenever a file
changes.

A plugin that links to another plugin's model (`"target": "currency.currency"`) needs that plugin loaded first;
list it under `dependencies`.

## Learn more

* `docs/plugin/rhai.md`: the language as Aether runs it, its limits and the commands a script can call.
* `docs/plugin/commands.md`: every kernel command, its capability and payload.
* `docs/plugin/pages.md` and `docs/plugin/cli-commands.md`: pages, and commands run with `aether --command`.
"#;

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
            ("Rhai", Language::Rhai),
            ("lua", Language::Lua),
            ("Luau", Language::Lua),
        ] {
            assert_eq!(raw.parse::<Language>()?, expected);
        }
        assert!("cobol".parse::<Language>().is_err());
        Ok(())
    }

    const WASM_LANGUAGES: [Language; 5] =
        [Language::Go, Language::Rust, Language::TypeScript, Language::JavaScript, Language::Python];

    #[test]
    fn every_language_has_sources_a_makefile_and_no_unfilled_placeholders() {
        for language in WASM_LANGUAGES.into_iter().chain([Language::Rhai, Language::Lua]) {
            let files = language.files("company");
            assert!(files.iter().any(|f| f.path.starts_with("src/") || Some(f.path) == language.script_file()));
            assert_eq!(files.iter().any(|f| f.path == "Makefile"), language.is_wasm(), "{language:?}");
            assert!(
                files.iter().all(|f| !f.content.contains("__NAME__")),
                "{language:?} left a placeholder"
            );
            assert!(!language.readme("company", "Company").contains("__"));
        }
    }

    #[test]
    fn makefiles_default_to_building_into_out_with_tab_recipes() {
        for language in WASM_LANGUAGES {
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
        for language in WASM_LANGUAGES {
            for file in language.files("company") {
                assert!(!file.content.contains("extism_pdk") && !file.content.contains("go-pdk") && !file.content.contains("@extism/js-pdk"), "{language:?} {}", file.path);
            }
        }
    }

    #[test]
    fn rhai_has_a_script_and_no_makefile_because_nothing_is_built() {
        let files = Language::Rhai.files("currency");
        let get = |path: &str| files.iter().find(|f| f.path == path).map(|f| f.content.clone()).unwrap_or_default();
        assert!(!Language::Rhai.is_wasm());
        assert_eq!(Language::Rhai.code_line(), "script = \"main.rhai\"");
        assert!(files.iter().all(|f| f.path != "Makefile"));

        let readme = Language::Rhai.readme("currency", "Currency");
        assert!(readme.contains("--install-plugin currency --org") && !readme.contains("make") && !readme.contains("plugin.wasm") && !readme.contains("__"));
        assert!(get(".gitignore").contains("*.swp"));
    }

    /// The sample is a plugin the kernel accepts: it compiles, and `hello` is callable.
    #[test]
    fn the_rhai_sample_compiles_and_exports_hello() -> Result<(), Box<dyn std::error::Error>> {
        let files = Language::Rhai.files("currency");
        let source = files.iter().find(|f| f.path == "main.rhai").map(|f| f.content.clone()).ok_or("no main.rhai")?;
        let program = aether_core::plugin_manager::script::ScriptProgram::compile(aether_core::plugin_manager::script::ScriptKind::Rhai, &source)?;
        assert!(program.has_function("hello"));
        assert!(source.contains("/api/plugins/currency/"));
        Ok(())
    }

    #[test]
    fn lua_has_a_script_and_no_makefile_because_nothing_is_built() {
        let files = Language::Lua.files("currency");
        let get = |path: &str| files.iter().find(|f| f.path == path).map(|f| f.content.clone()).unwrap_or_default();
        assert!(!Language::Lua.is_wasm());
        assert_eq!(Language::Lua.code_line(), "script = \"main.lua\"");
        assert!(files.iter().all(|f| f.path != "Makefile"));

        let readme = Language::Lua.readme("currency", "Currency");
        assert!(readme.contains("--install-plugin currency --org") && !readme.contains("make") && !readme.contains("plugin.wasm") && !readme.contains("__"));
        assert!(readme.contains("main.lua") && readme.contains("docs/plugin/lua.md"));
        assert!(get(".gitignore").contains("*.swp"));
    }

    /// The sample is a plugin the kernel accepts: it compiles, and `hello` is callable.
    #[test]
    fn the_lua_sample_compiles_and_exports_hello() -> Result<(), Box<dyn std::error::Error>> {
        use aether_core::plugin_manager::script::{ScriptKind, ScriptProgram};
        let files = Language::Lua.files("currency");
        let source = files.iter().find(|f| f.path == "main.lua").map(|f| f.content.clone()).ok_or("no main.lua")?;
        let program = ScriptProgram::compile(ScriptKind::Lua, &source)?;
        assert!(program.has_function("hello"));
        assert!(source.contains("/api/plugins/currency/"));
        Ok(())
    }
}
