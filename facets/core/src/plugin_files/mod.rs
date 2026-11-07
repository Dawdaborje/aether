//! Files a plugin keeps. The path rules and the watch engine are in `aether_kernel::plugin_files`;
//! the watcher service that needs the server state is here.

pub use aether_kernel::plugin_files::*;

mod manager;

pub mod watch {
    pub use aether_kernel::plugin_files::watch::*;

    pub use super::manager::WatchManager;
}
