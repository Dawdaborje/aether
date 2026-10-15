//! Report specs and file export.
//!
//! The kernel parses a [`ReportQuery`] here, then runs it against the organization
//! database. [`export`] turns rows the plugin already has into an Excel or PDF table.

mod export;
mod spec;

pub use export::{ExportColumn, ExportFile, ExportFormat, ExportTable, export_table, parse_export};
pub use spec::{
    AvgDaysTo, Dimension, Fill, Grain, Measure, ReportError, ReportQuery, PREVIEW_CHOICES,
    PREVIEW_DEFAULT, normalize_preview_rows,
};
