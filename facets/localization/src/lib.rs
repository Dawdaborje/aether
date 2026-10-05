//! Localization: which language a person reads, and the text in it.
//!
//! The core interface is translated in the browser (Paraglide). Everything a plugin shows is
//! translated here, in the kernel, because plugins are installed at run time and their text
//! cannot be part of the web build. See `docs/architecture/localization.md`.
//!
//! - [`locale`]: locale names, the fallback chain, `Accept-Language`, right-to-left.
//! - [`format`]: the message syntax (`{name}`, `plural`, `select`).
//! - [`catalog`]: plugin catalogs, overrides, and looking a key up.
//! - [`message`]: a message a function returns (`{ key, params, default }`) and `%key%` in page trees.

pub mod catalog;
pub mod format;
pub mod locale;
pub mod message;

pub use catalog::{Catalogs, Translator};
pub use format::{FormatError, format_message};
pub use locale::{fallback_chain, is_rtl, normalize, parse_accept_language};
pub use message::{Message, resolve_tree};
