//! Importing records from a spreadsheet file (see `docs/architecture/import_export.md`).
//!
//! * [`table`]: a CSV or XLSX file as rows of text.
//! * [`mapping`]: which column is which field, and how a cell's text becomes the field's value.
//! * [`rows`]: applying the rows to a model, one decision per row.
//! * [`job`]: an import as a scheduler job: slices, progress, the file of failed rows, undo.

pub use aether_kernel::import::{mapping, rows, table};

pub mod job;
