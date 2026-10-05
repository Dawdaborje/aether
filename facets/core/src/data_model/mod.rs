//! Data models: what a plugin's records look like, kept apart from what things are called.
//!
//! A plugin describes each model in `models/<name>.json`. Every model and every field has a
//! stable **id** (`mdl_…`, `fld_…`) next to its human **name**. Names are what plugin code and
//! pages use and may change freely; ids are what the database stores, so:
//!
//! * renaming a field, or changing its label, only edits the definition: no column is added,
//!   nothing is copied, no migration is written;
//! * organizations on different versions of a plugin can share a table, because the version
//!   that calls it `title` and the one that calls it `name` mean the same column;
//! * removing a field hides it instead of deleting its data, and its id is never reused.
//!
//! [`definition`] is the file format and its rules, [`query`] turns filters and aggregates into queries, [`runtime`] translates between a plugin's
//! names and the stored ids (and checks every value), [`apply`] works out and runs the change
//! an upgrade needs in an organization's database.

pub mod apply;
pub mod decimal;
pub mod definition;
pub mod query;
pub mod rules;
pub mod runtime;

pub use definition::{
    ChatterDef, FieldDef, FieldType, IndexDef, IndexKind, ModelDef, ModelFileError, SelectOption, ViewDef, new_id,
    parse_model, read_models, sync_ids, sync_package, sync_package_with, validate_set, VisitorChatter, foreign_target,
    foreign_targets,
};
pub use query::{Agg, Aggregate, Filter, QueryError};
pub use rules::{Decision, Operation, RuleSet};
pub use runtime::{Hierarchy, ModelSchema, Relation, SchemaError, edge_table, schemas_of};
