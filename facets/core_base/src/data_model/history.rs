//! Reading the past from a record's versions (see `docs/architecture/history.md`).
//!
//! The versions of one record are rows of `record_versions`: a full `snapshot` now and then, and a
//! `delta` (fields that changed to a value) with `cleared` (fields that became empty) for each
//! update. Everything here is pure: the caller loads the rows, this turns them into the record as it
//! was, or into what changed between two versions.

use serde_json::{Map, Value};

/// One row of `record_versions`, as far as reading needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Version {
    pub seq: u64,
    /// `baseline`, `create`, `update` or `delete`.
    pub op: String,
    pub snapshot: Option<Map<String, Value>>,
    pub delta: Option<Map<String, Value>>,
    pub cleared: Vec<String>,
}

impl Version {
    /// Read a row as the database returns it. `None` if it lacks `seq` or `op`.
    pub fn from_row(row: &Value) -> Option<Self> {
        let object = |key: &str| row.get(key).and_then(Value::as_object).cloned();
        Some(Self {
            seq: row.get("seq")?.as_u64()?,
            op: row.get("op")?.as_str()?.to_string(),
            snapshot: object("snapshot"),
            delta: object("delta"),
            cleared: row
                .get("cleared")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    pub fn is_delete(&self) -> bool {
        self.op == "delete"
    }
}

/// The record as it was right after version `seq`, by stored field id; `None` when it did not
/// exist then (before its first version, or deleted by `seq`). `versions` must be sorted by `seq`
/// and reach back to a snapshot at or before `seq`; without one the state cannot be known and the
/// answer is `None`.
pub fn state_at(versions: &[Version], seq: u64) -> Option<Map<String, Value>> {
    let upto: Vec<&Version> = versions.iter().filter(|v| v.seq <= seq).collect();
    let last = upto.last()?;
    if last.is_delete() {
        return None;
    }
    let start = upto.iter().rposition(|v| v.snapshot.is_some())?;
    let mut state = upto[start].snapshot.clone()?;
    for version in &upto[start + 1..] {
        if version.is_delete() {
            return None;
        }
        apply(&mut state, version);
    }
    Some(state)
}

fn apply(state: &mut Map<String, Value>, version: &Version) {
    if let Some(delta) = &version.delta {
        for (key, value) in delta {
            state.insert(key.clone(), value.clone());
        }
    }
    for key in &version.cleared {
        state.remove(key);
    }
}

/// The fields a version changed, by stored id: its delta and cleared fields, or for the first
/// version of a record (a snapshot with no delta) every field it holds.
pub fn changed_fields(version: &Version) -> Vec<String> {
    let mut fields: Vec<String> = match (&version.delta, &version.snapshot) {
        (Some(delta), _) => delta.keys().cloned().collect(),
        (None, Some(snapshot)) => snapshot.keys().filter(|key| *key != "id").cloned().collect(),
        (None, None) => Vec::new(),
    };
    fields.extend(version.cleared.iter().cloned());
    fields.sort();
    fields.dedup();
    fields
}

/// One field that differs between two states: `before`/`after` are `None` where it was empty.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldChange {
    pub field: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

/// What differs between two states of a record, sorted by field id. `id` is never a change.
pub fn diff(before: &Map<String, Value>, after: &Map<String, Value>) -> Vec<FieldChange> {
    let mut keys: Vec<&String> = before.keys().chain(after.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter(|key| key.as_str() != "id")
        .filter_map(|key| {
            let (b, a) = (before.get(key), after.get(key));
            (b != a).then(|| FieldChange {
                field: key.clone(),
                before: b.cloned(),
                after: a.cloned(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(value: Value) -> Version {
        Version::from_row(&value).expect("a version row")
    }

    fn history() -> Vec<Version> {
        vec![
            row(json!({ "seq": 1, "op": "create", "snapshot": { "id": "t:1", "a": 1, "b": "x" } })),
            row(json!({ "seq": 2, "op": "update", "delta": { "a": 2 } })),
            row(json!({ "seq": 3, "op": "update", "delta": { "c": true }, "cleared": ["b"] })),
            // A snapshot version also keeps its delta.
            row(json!({ "seq": 4, "op": "update", "delta": { "a": 9 },
                       "snapshot": { "id": "t:1", "a": 9, "c": true } })),
            row(json!({ "seq": 5, "op": "delete" })),
        ]
    }

    fn state(seq: u64) -> Option<Value> {
        state_at(&history(), seq).map(Value::Object)
    }

    #[test]
    fn a_state_is_the_snapshot_with_the_deltas_after_it() {
        assert_eq!(state(1), Some(json!({ "id": "t:1", "a": 1, "b": "x" })));
        assert_eq!(state(2), Some(json!({ "id": "t:1", "a": 2, "b": "x" })));
        assert_eq!(state(3), Some(json!({ "id": "t:1", "a": 2, "c": true })));
        assert_eq!(state(4), Some(json!({ "id": "t:1", "a": 9, "c": true })));
    }

    #[test]
    fn nothing_exists_before_the_first_version_or_after_a_delete() {
        assert_eq!(state(0), None);
        assert_eq!(state(5), None);
        assert_eq!(state(99), None, "still deleted later");
    }

    #[test]
    fn without_a_snapshot_to_start_from_the_state_is_unknown() {
        let versions = vec![row(json!({ "seq": 2, "op": "update", "delta": { "a": 2 } }))];
        assert_eq!(state_at(&versions, 2), None);
    }

    #[test]
    fn a_window_starting_at_a_later_snapshot_gives_the_same_state() {
        let all = history();
        assert_eq!(state_at(&all[3..4], 4), state_at(&all, 4));
    }

    #[test]
    fn changed_fields_name_the_delta_the_cleared_and_everything_for_a_first_version() {
        let h = history();
        assert_eq!(changed_fields(&h[0]), vec!["a", "b"]);
        assert_eq!(changed_fields(&h[1]), vec!["a"]);
        assert_eq!(changed_fields(&h[2]), vec!["b", "c"]);
        assert_eq!(changed_fields(&h[3]), vec!["a"], "the delta, not the snapshot");
        assert!(changed_fields(&h[4]).is_empty());
    }

    #[test]
    fn a_diff_lists_what_differs_and_ignores_the_id() {
        let (a, b) = (
            state_at(&history(), 1).unwrap_or_default(),
            state_at(&history(), 3).unwrap_or_default(),
        );
        let changes = diff(&a, &b);
        let fields: Vec<&str> = changes.iter().map(|c| c.field.as_str()).collect();
        assert_eq!(fields, vec!["a", "b", "c"]);
        assert_eq!(changes[1].before, Some(json!("x")));
        assert_eq!(changes[1].after, None, "cleared");
        assert_eq!(changes[2].before, None, "new");
        assert!(diff(&a, &a).is_empty());
    }
}
