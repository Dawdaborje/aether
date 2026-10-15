//! The functions a plugin's code offers, read from the code itself.
//!
//! A WebAssembly plugin offers the functions its module exports; a Rhai or Lua plugin, the
//! ones its script defines. The manifest says which of those are public, commands, scheduled
//! or event handlers; this reads the other half, so Desk can show a developer what a plugin
//! can be called with and spot a manifest that names a function the code does not have.
//!
//! Nothing here runs the plugin: a module is only read as bytes.

/// Reads one unsigned LEB128 integer (at most 32 bits) and the offset after it.
fn leb(bytes: &[u8], mut at: usize) -> Option<(u32, usize)> {
    let mut value: u32 = 0;
    let mut shift = 0;
    loop {
        let byte = *bytes.get(at)?;
        at += 1;
        if shift >= 35 {
            return None;
        }
        value |= u32::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some((value, at));
        }
        shift += 7;
    }
}

/// Names of the functions a WebAssembly module exports, in the order it lists them.
/// Toolchain internals (`_start`, `__wasm_call_ctors`, …) are left out. `None` when the
/// bytes are not a module.
pub fn wasm_exports(bytes: &[u8]) -> Option<Vec<String>> {
    if bytes.get(..4)? != b"\0asm" || bytes.len() < 8 {
        return None;
    }
    let mut at = 8;
    while at < bytes.len() {
        let id = *bytes.get(at)?;
        let (size, body) = leb(bytes, at + 1)?;
        let end = body.checked_add(size as usize)?;
        if end > bytes.len() {
            return None;
        }
        if id == 7 {
            let (count, mut cursor) = leb(bytes, body)?;
            let mut names = Vec::new();
            for _ in 0..count {
                let (length, name_at) = leb(bytes, cursor)?;
                let name_end = name_at.checked_add(length as usize)?;
                let name = std::str::from_utf8(bytes.get(name_at..name_end)?).ok()?;
                let kind = *bytes.get(name_end)?;
                let (_, next) = leb(bytes, name_end + 1)?;
                cursor = next;
                // Kind 0 is a function; memories, tables and globals are not callable.
                if kind == 0 && !name.starts_with('_') {
                    names.push(name.to_string());
                }
            }
            return Some(names);
        }
        at = end;
    }
    Some(Vec::new())
}

/// Functions a Rhai script defines for callers: `fn name(…)`, not `private fn`.
pub fn rhai_functions(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in source.lines() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix("fn ") else {
            continue;
        };
        if let Some(name) = identifier_before_paren(rest) {
            if !found.contains(&name) {
                found.push(name);
            }
        }
    }
    found
}

/// Functions a Lua script defines at its top level: `function name(…)`, not `local function`
/// and not a method of a table (`function M.name`).
pub fn lua_functions(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in source.lines() {
        let Some(rest) = line.strip_prefix("function ") else {
            continue;
        };
        if let Some(name) = identifier_before_paren(rest) {
            if !found.contains(&name) {
                found.push(name);
            }
        }
    }
    found
}

fn identifier_before_paren(text: &str) -> Option<String> {
    let name: String = text
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let after = text.trim_start()[name.len()..].trim_start();
    after.starts_with('(').then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `(module (func (export "ping")) (func (export "_start")) (memory (export "memory") 1))`
    fn module() -> Vec<u8> {
        let mut m = b"\0asm\x01\0\0\0".to_vec();
        // type section: one type () -> ()
        m.extend([1, 4, 1, 0x60, 0, 0]);
        // function section: two functions of type 0
        m.extend([3, 3, 2, 0, 0]);
        // memory section: one memory, min 1
        m.extend([5, 3, 1, 0, 1]);
        // export section
        let mut exports = vec![3u8];
        for (name, kind, index) in [("ping", 0u8, 0u8), ("_start", 0, 1), ("memory", 2, 0)] {
            exports.push(name.len() as u8);
            exports.extend(name.as_bytes());
            exports.extend([kind, index]);
        }
        m.push(7);
        m.push(exports.len() as u8);
        m.extend(exports);
        // code section: two empty bodies
        m.extend([10, 7, 2, 2, 0, 0x0b, 2, 0, 0x0b]);
        m
    }

    #[test]
    fn lists_exported_functions_without_internals_or_memory() {
        assert_eq!(wasm_exports(&module()), Some(vec!["ping".to_string()]));
    }

    #[test]
    fn refuses_bytes_that_are_not_a_module() {
        assert_eq!(wasm_exports(b"not wasm at all"), None);
        assert_eq!(wasm_exports(b"\0asm\x01\0\0\0\x07\xff"), None);
    }

    #[test]
    fn a_module_with_no_exports_has_no_functions() {
        assert_eq!(wasm_exports(b"\0asm\x01\0\0\0"), Some(vec![]));
    }

    #[test]
    fn reads_rhai_functions_and_skips_private_ones() {
        let source = "fn create(input) {\n  1\n}\nprivate fn helper(x) { x }\n  fn  list_all (input) { }\nlet fn_x = 1;\n";
        assert_eq!(rhai_functions(source), vec!["create", "list_all"]);
    }

    #[test]
    fn reads_top_level_lua_functions_only() {
        let source = "local function helper() end\nfunction create(input)\nend\nfunction M.method() end\nfunction list(i) end\n";
        assert_eq!(lua_functions(source), vec!["create", "list"]);
    }
}
