pub mod args;
pub mod audit;
pub mod db;
pub mod generation;
pub mod initialization;
pub mod logging;
pub mod organization;
pub mod palette;
pub mod commands;
pub mod plugin_manager;
pub mod runner;
pub mod scaffold;
pub mod seed;
pub mod setup_mcp;
pub mod uninstall;

pub use runner::run;
