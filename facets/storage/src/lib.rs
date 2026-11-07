//! Media storage contract.
//!
//! Every storage integration (local disk, S3-compatible object stores, and
//! later Nextcloud, Google Drive, …) implements [`MediaBackend`]. The kernel
//! holds one backend, selected by `[media]` in `aether.toml`, and plugins reach
//! it only through kernel commands, never directly.

mod adapter;
mod backend;
mod error;
mod fallback;
mod key;
mod prefixed;

pub use adapter::ObjectStoreBackend;
pub use backend::{MediaBackend, MediaObject};
pub use error::StorageError;
pub use fallback::{DrainReport, FallbackBackend, PendingEntry, PendingJournal};
pub use key::MediaKey;
pub use prefixed::{PrefixedBackend, org_media_prefix};
