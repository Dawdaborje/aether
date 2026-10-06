//! Who may see and change which records and fields.
//!
//! A plugin describes it in `rules/<model>.json`, one file per model:
//!
//! ```json
//! {
//!   "model": "leave_request",
//!   "access": [
//!     { "name": "own",       "operations": ["read", "create", "write", "delete"], "when": { "employee": "$hr.employee" } },
//!     { "name": "approvers", "roles": ["approver"], "operations": ["read", "write"],
//!       "when": { "employee": { "in": "$hr.subordinates" } } },
//!     { "name": "hr",        "roles": ["hr.hr_manager"], "operations": ["read", "write"] }
//!   ],
//!   "restrict": [
//!     { "name": "not_after_approval", "operations": ["write", "delete"], "exempt_roles": ["hr.hr_manager"],
//!       "when": { "state": { "in": ["submitted"] } } }
//!   ],
//!   "fields": [
//!     { "fields": ["state", "decided_by"], "write_roles": ["approver", "hr.hr_manager"] },
//!     { "fields": ["wage"], "read_roles": ["hr.hr_manager"] }
//!   ]
//! }
//! ```
//!
//! * `access` rows **grant**. For an operation, the person gets the records matching *any* grant
//!   that applies to them (`roles` empty: every member; otherwise a person holding one of the roles)
//!   and names the operation. A model with no grant for an operation is open for it; once it has one,
//!   a person none of its grants applies to gets nothing. No `when` means every record.
//! * `restrict` rows **narrow**: the record must also match every restriction that applies, except for
//!   people holding an `exempt_roles` role.
//! * `fields` say which roles may read or write fields; everyone else cannot see them, or is refused
//!   when they try to set them. A field not mentioned is open.
//! * Administrators (`org_admin`) and the kernel itself are not restricted.
//!
//! Role names are `<plugin>.<role>`; a bare name means a role of the plugin that owns the file.
//! In a `when`, a string starting with `$` is a **variable**: `$user` is the caller's account, and
//! `$<plugin>.<name>` is declared by a plugin (`[[variables]]` in its manifest) as a function that
//! answers for the caller, such as their employee record. A variable that has no value for the
//! caller makes the comparison match nothing.
//!
//! One definition gives both the filter for lists and counts and the check for a single record
//! being written, so the two cannot disagree.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::definition::ModelDef;
use super::query::{Filter, Op, QueryError};

/// Where a plugin's rule files are, relative to the package.
pub const RULE_DIR: &str = "rules";

/// Read and check a plugin's rule files against its models. Every problem found is reported.
pub fn parse_all(files: &[(String, String)], models: &[ModelDef]) -> Result<Vec<RuleSet>, Vec<String>> {
    let mut problems = Vec::new();
    let mut sets = Vec::new();
    for (name, text) in files {
        match RuleSet::parse(text) {
            Err(error) => problems.push(format!("{name}: {error}")),
            Ok(set) => {
                match models.iter().find(|model| model.name == set.model) {
                    None => problems.push(format!("{name}: there is no model `{}` in this plugin", set.model)),
                    Some(model) => problems.extend(set.problems(model).into_iter().map(|p| format!("{name}: {p}"))),
                }
                if sets.iter().any(|other: &RuleSet| other.model == set.model) {
                    problems.push(format!("{name}: `{}` already has a rule file", set.model));
                }
                sets.push(set);
            }
        }
    }
    if problems.is_empty() { Ok(sets) } else { Err(problems) }
}

/// What can be done to a record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Read,
    Create,
    Write,
    Delete,
}

impl Operation {
    pub const ALL: [Operation; 4] = [Operation::Read, Operation::Create, Operation::Write, Operation::Delete];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Create => "create",
            Self::Write => "write",
            Self::Delete => "delete",
        }
    }
}

fn all_operations() -> Vec<Operation> {
    Operation::ALL.to_vec()
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AccessRule {
    pub name: String,
    #[serde(default)]
    pub roles: Vec<String>,
    pub operations: Vec<Operation>,
    #[serde(default)]
    pub when: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RestrictRule {
    pub name: String,
    #[serde(default)]
    pub exempt_roles: Vec<String>,
    #[serde(default = "all_operations")]
    pub operations: Vec<Operation>,
    pub when: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldRule {
    pub fields: Vec<String>,
    /// Who may read these fields; none listed: everyone.
    #[serde(default)]
    pub read_roles: Option<Vec<String>>,
    /// Who may set these fields; none listed: everyone.
    #[serde(default)]
    pub write_roles: Option<Vec<String>>,
}

/// One move of a record from a state to another.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    pub name: String,
    /// Shown instead of the name.
    #[serde(default)]
    pub label: Option<String>,
    /// The states the record may be in. Never empty.
    pub from: Vec<String>,
    pub to: String,
    /// Who may make the move; none listed: anyone who may change the record.
    #[serde(default)]
    pub roles: Vec<String>,
    /// What the record must match (before the move), like a `restrict` row's `when`.
    #[serde(default)]
    pub when: Option<Map<String, Value>>,
}

/// The states of a model and the moves between them. A write that changes the `field` is allowed
/// only as one of the `transitions` the caller may make; a new record starts in `initial`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Workflow {
    /// A `select` field of the model.
    pub field: String,
    /// The states a record may be created in. Default: the field's default.
    #[serde(default)]
    pub initial: Vec<String>,
    pub transitions: Vec<Transition>,
}

/// The rules of one model.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuleSet {
    pub model: String,
    #[serde(default)]
    pub access: Vec<AccessRule>,
    #[serde(default)]
    pub restrict: Vec<RestrictRule>,
    #[serde(default)]
    pub fields: Vec<FieldRule>,
    #[serde(default)]
    pub workflow: Option<Workflow>,
}

/// What the rules say about one operation for one person.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Nothing limits it.
    Open,
    /// Only records matching the filter.
    Only(Filter),
    /// Nothing at all.
    Deny,
}

/// A role name made absolute: `approver` in plugin `hr_leave` is `hr_leave.approver`.
pub fn qualify(plugin: &str, role: &str) -> String {
    if role.contains('.') || role == "org_admin" || role.starts_with("via:") { role.to_string() } else { format!("{plugin}.{role}") }
}

fn holds(plugin: &str, held: &[String], wanted: &[String]) -> bool {
    wanted.iter().any(|role| held.iter().any(|h| *h == qualify(plugin, role)))
}

impl RuleSet {
    /// Read a rule file.
    pub fn parse(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    fn grants_for(&self, plugin: &str, op: Operation, roles: &[String]) -> Vec<&AccessRule> {
        self.access
            .iter()
            .filter(|rule| rule.operations.contains(&op))
            .filter(|rule| rule.roles.is_empty() || holds(plugin, roles, &rule.roles))
            .collect()
    }

    fn restrictions_for(&self, plugin: &str, op: Operation, roles: &[String]) -> Vec<&RestrictRule> {
        self.restrict
            .iter()
            .filter(|rule| rule.operations.contains(&op))
            .filter(|rule| rule.exempt_roles.is_empty() || !holds(plugin, roles, &rule.exempt_roles))
            .collect()
    }

    /// The variables the rules that apply to this person and operation need answered first.
    pub fn variables_needed(&self, plugin: &str, op: Operation, roles: &[String]) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        let grants = self.grants_for(plugin, op, roles);
        let whens = grants
            .iter()
            .filter_map(|rule| rule.when.as_ref())
            .chain(self.restrictions_for(plugin, op, roles).into_iter().map(|rule| &rule.when));
        for when in whens {
            collect_variables(&Value::Object(when.clone()), &mut found);
        }
        found
    }

    /// Whether anything limits this operation at all (a model with no rules for it is open).
    pub fn limits(&self, op: Operation) -> bool {
        self.access.iter().any(|rule| rule.operations.contains(&op))
            || self.restrict.iter().any(|rule| rule.operations.contains(&op))
    }

    /// What the rules allow `roles` to do with `op`. `variables` has the answer for every name
    /// [`variables_needed`](Self::variables_needed) listed; a missing or null one matches nothing.
    pub fn decide(
        &self,
        plugin: &str,
        op: Operation,
        roles: &[String],
        variables: &HashMap<String, Value>,
    ) -> Result<Decision, QueryError> {
        let has_grants = self.access.iter().any(|rule| rule.operations.contains(&op));
        let mut allowed = if has_grants {
            let applicable = self.grants_for(plugin, op, roles);
            if applicable.is_empty() {
                return Ok(Decision::Deny);
            }
            let mut any = Filter::Never;
            for rule in applicable {
                let filter = match &rule.when {
                    Some(when) => bind(Filter::parse(when)?, variables),
                    None => Filter::always(),
                };
                any = any.or(filter);
            }
            any
        } else {
            Filter::always()
        };
        for rule in self.restrictions_for(plugin, op, roles) {
            allowed = allowed.and(bind(Filter::parse(&rule.when)?, variables));
        }
        if allowed == Filter::Never {
            return Ok(Decision::Deny);
        }
        Ok(if allowed.is_empty() { Decision::Open } else { Decision::Only(allowed) })
    }

    /// The transitions `roles` may make, whatever the record looks like.
    fn transitions_for(&self, plugin: &str, roles: &[String]) -> Vec<&Transition> {
        match &self.workflow {
            Some(workflow) => workflow
                .transitions
                .iter()
                .filter(|t| t.roles.is_empty() || holds(plugin, roles, &t.roles))
                .collect(),
            None => Vec::new(),
        }
    }

    /// The variables the transitions that `roles` may make need answered first.
    pub fn workflow_variables(&self, plugin: &str, roles: &[String]) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        for transition in self.transitions_for(plugin, roles) {
            if let Some(when) = &transition.when {
                collect_variables(&Value::Object(when.clone()), &mut found);
            }
        }
        found
    }

    /// The transitions `roles` may make, each with the condition a record must meet for the move
    /// (its state is one of the `from` states, and the transition's `when` holds). A `when` that
    /// depends on a variable the caller has no value for makes the move impossible.
    pub fn available_moves(
        &self,
        plugin: &str,
        roles: &[String],
        variables: &HashMap<String, Value>,
    ) -> Result<Vec<(&Transition, Filter)>, QueryError> {
        let Some(workflow) = &self.workflow else { return Ok(Vec::new()) };
        let mut out = Vec::new();
        for transition in self.transitions_for(plugin, roles) {
            let from = Filter::Cmp {
                field: workflow.field.clone(),
                op: Op::In,
                value: Value::Array(transition.from.iter().map(|s| Value::String(s.clone())).collect()),
            };
            let when = match &transition.when {
                Some(when) => bind(Filter::parse(when)?, variables),
                None => Filter::always(),
            };
            let condition = simplify(from.and(when));
            if condition != Filter::Never {
                out.push((transition, condition));
            }
        }
        Ok(out)
    }

    /// What `roles` may do to move a record to `to`: records already in that state are fine, and
    /// so are records a transition into it applies to. `Deny` when no transition applies to them.
    pub fn move_to(
        &self,
        plugin: &str,
        to: &str,
        roles: &[String],
        variables: &HashMap<String, Value>,
    ) -> Result<Decision, QueryError> {
        let Some(workflow) = &self.workflow else { return Ok(Decision::Open) };
        let mut allowed = Filter::Never;
        for (transition, condition) in self.available_moves(plugin, roles, variables)? {
            if transition.to == to {
                allowed = allowed.or(condition);
            }
        }
        if allowed == Filter::Never {
            return Ok(Decision::Deny);
        }
        let unchanged = Filter::Cmp { field: workflow.field.clone(), op: Op::Eq, value: Value::String(to.to_string()) };
        Ok(Decision::Only(unchanged.or(allowed)))
    }

    /// Fields `roles` may not read.
    pub fn hidden_fields(&self, plugin: &str, roles: &[String]) -> HashSet<&str> {
        self.fields
            .iter()
            .filter(|rule| rule.read_roles.as_ref().is_some_and(|wanted| !holds(plugin, roles, wanted)))
            .flat_map(|rule| rule.fields.iter().map(String::as_str))
            .collect()
    }

    /// Fields `roles` may not set.
    pub fn locked_fields(&self, plugin: &str, roles: &[String]) -> HashSet<&str> {
        self.fields
            .iter()
            .filter(|rule| rule.write_roles.as_ref().is_some_and(|wanted| !holds(plugin, roles, wanted)))
            .flat_map(|rule| rule.fields.iter().map(String::as_str))
            .collect()
    }

    /// What is wrong with the rules, given the model they are for. Empty when they are fine.
    pub fn problems(&self, model: &ModelDef) -> Vec<String> {
        let mut problems = Vec::new();
        let known = |name: &str| name == "id" || model.live_fields().any(|field| field.name == name);
        let mut names = HashSet::new();
        for rule in &self.access {
            if !names.insert(rule.name.as_str()) {
                problems.push(format!("{}: two rules are named `{}`", self.model, rule.name));
            }
            if rule.operations.is_empty() {
                problems.push(format!("{}: rule `{}` names no operations", self.model, rule.name));
            }
            if let Some(when) = &rule.when {
                self.check_when(&rule.name, when, &known, &mut problems);
            }
        }
        for rule in &self.restrict {
            if !names.insert(rule.name.as_str()) {
                problems.push(format!("{}: two rules are named `{}`", self.model, rule.name));
            }
            if rule.operations.is_empty() {
                problems.push(format!("{}: rule `{}` names no operations", self.model, rule.name));
            }
            self.check_when(&rule.name, &rule.when, &known, &mut problems);
        }
        if let Some(workflow) = &self.workflow {
            self.workflow_problems(workflow, model, &known, &mut problems);
        }
        for rule in &self.fields {
            if rule.fields.is_empty() {
                problems.push(format!("{}: a field rule names no fields", self.model));
            }
            if rule.read_roles.is_none() && rule.write_roles.is_none() {
                problems.push(format!("{}: a field rule limits neither reading nor writing", self.model));
            }
            for field in &rule.fields {
                if !known(field) || field == "id" {
                    problems.push(format!("{}: field rule names `{field}`, which is not a field", self.model));
                }
            }
        }
        problems
    }

    fn workflow_problems(&self, workflow: &Workflow, model: &ModelDef, known: &dyn Fn(&str) -> bool, problems: &mut Vec<String>) {
        let name = &self.model;
        let Some(field) = model.live_fields().find(|f| f.name == workflow.field) else {
            problems.push(format!("{name}: the workflow's field `{}` is not a field", workflow.field));
            return;
        };
        if field.kind != super::definition::FieldType::Select {
            problems.push(format!("{name}: the workflow's field `{}` must be a select field", workflow.field));
            return;
        }
        let states: HashSet<&str> = field.options.iter().map(|o| o.value.as_str()).collect();
        let state = |text: &str, what: &str, problems: &mut Vec<String>| {
            if !states.contains(text) {
                problems.push(format!("{name}: the workflow {what} `{text}`, which is not an option of `{}`", workflow.field));
            }
        };
        let initial: Vec<String> = if workflow.initial.is_empty() {
            field.default.as_ref().and_then(Value::as_str).map(|d| vec![d.to_string()]).unwrap_or_default()
        } else {
            workflow.initial.clone()
        };
        if initial.is_empty() {
            problems.push(format!("{name}: the workflow needs `initial` states (or the field a `default`)"));
        }
        for text in &initial {
            state(text, "starts in", problems);
        }
        if workflow.transitions.is_empty() {
            problems.push(format!("{name}: the workflow has no transitions"));
        }
        let mut names = HashSet::new();
        for transition in &workflow.transitions {
            if !names.insert(transition.name.as_str()) {
                problems.push(format!("{name}: two transitions are named `{}`", transition.name));
            }
            if transition.from.is_empty() {
                problems.push(format!("{name}: transition `{}` has no `from` states", transition.name));
            }
            for text in &transition.from {
                state(text, &format!("moves `{}` from", transition.name), problems);
            }
            state(&transition.to, &format!("moves `{}` to", transition.name), problems);
            if let Some(when) = &transition.when {
                self.check_when(&format!("transition {}", transition.name), when, known, problems);
            }
        }
    }

    fn check_when(&self, rule: &str, when: &Map<String, Value>, known: &dyn Fn(&str) -> bool, problems: &mut Vec<String>) {
        match Filter::parse(when) {
            Err(error) => problems.push(format!("{}: rule `{rule}`: {error}", self.model)),
            Ok(filter) => {
                for field in filter.fields() {
                    if !known(field) {
                        problems.push(format!("{}: rule `{rule}` compares `{field}`, which is not a field", self.model));
                    }
                }
            }
        }
    }
}

fn collect_variables(value: &Value, found: &mut BTreeSet<String>) {
    match value {
        Value::String(text) => {
            if let Some(name) = variable_name(text) {
                found.insert(name.to_string());
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect_variables(item, found)),
        Value::Object(map) => map.values().for_each(|item| collect_variables(item, found)),
        _ => {}
    }
}

/// `$name` is a variable; `$$name` is the text `$name`.
fn variable_name(text: &str) -> Option<&str> {
    text.strip_prefix('$').filter(|rest| !rest.is_empty() && !rest.starts_with('$'))
}

/// A filter with its variables replaced by their values and the obvious simplified away: an `and`
/// holding a filter that matches nothing matches nothing, a list of one is that one.
fn bind(filter: Filter, variables: &HashMap<String, Value>) -> Filter {
    simplify(bind_variables(filter, variables))
}

fn simplify(filter: Filter) -> Filter {
    match filter {
        Filter::And(parts) => {
            let mut kept = Vec::new();
            for part in parts.into_iter().map(simplify) {
                match part {
                    Filter::Never => return Filter::Never,
                    part if part.is_empty() => {}
                    part => kept.push(part),
                }
            }
            match kept.len() {
                0 => Filter::always(),
                1 => kept.remove(0),
                _ => Filter::And(kept),
            }
        }
        Filter::Or(parts) => {
            let mut kept = Vec::new();
            for part in parts.into_iter().map(simplify) {
                match part {
                    Filter::Never => {}
                    part if part.is_empty() => return Filter::always(),
                    part => kept.push(part),
                }
            }
            match kept.len() {
                0 => Filter::Never,
                1 => kept.remove(0),
                _ => Filter::Or(kept),
            }
        }
        Filter::Not(inner) => match simplify(*inner) {
            Filter::Never => Filter::always(),
            inner if inner.is_empty() => Filter::Never,
            inner => Filter::Not(Box::new(inner)),
        },
        other => other,
    }
}

fn bind_variables(filter: Filter, variables: &HashMap<String, Value>) -> Filter {
    match filter {
        Filter::And(parts) => Filter::And(parts.into_iter().map(|p| bind_variables(p, variables)).collect()),
        Filter::Or(parts) => Filter::Or(parts.into_iter().map(|p| bind_variables(p, variables)).collect()),
        Filter::Not(inner) => Filter::Not(Box::new(bind_variables(*inner, variables))),
        Filter::Never => Filter::Never,
        Filter::Cmp { field, op, value } => {
            let name = match &value {
                Value::String(text) => variable_name(text).map(str::to_string),
                _ => None,
            };
            let Some(name) = name else {
                // `$$x` is a literal `$x`.
                let value = match value {
                    Value::String(text) if text.starts_with("$$") => Value::String(text[1..].to_string()),
                    other => other,
                };
                return Filter::Cmp { field, op, value };
            };
            match variables.get(&name) {
                None | Some(Value::Null) => Filter::Never,
                Some(found) => {
                    // A list stands for "one of", whatever comparison was written.
                    let (op, value) = match (op, found) {
                        (Op::Eq, Value::Array(_)) => (Op::In, found.clone()),
                        (Op::Ne, Value::Array(_)) => (Op::Nin, found.clone()),
                        _ => (op, found.clone()),
                    };
                    match (&value, op) {
                        (Value::Array(items), Op::In) if items.is_empty() => Filter::Never,
                        _ => Filter::Cmp { field, op, value },
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn rules(value: Value) -> RuleSet {
        serde_json::from_value(value).unwrap_or_else(|e| panic!("{e}"))
    }

    fn roles(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_string()).collect()
    }

    fn vars(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
    }

    fn leave() -> RuleSet {
        rules(json!({
            "model": "leave_request",
            "access": [
                { "name": "own", "operations": ["read", "create", "write", "delete"], "when": { "employee": "$hr.employee" } },
                { "name": "approvers", "roles": ["approver"], "operations": ["read", "write"],
                  "when": { "employee": { "in": "$hr.subordinates" } } },
                { "name": "hr", "roles": ["hr.hr_manager"], "operations": ["read", "write"] }
            ],
            "restrict": [
                { "name": "pending_only", "operations": ["write", "delete"], "exempt_roles": ["hr.hr_manager"],
                  "when": { "state": "submitted" } }
            ],
            "fields": [
                { "fields": ["state"], "write_roles": ["approver", "hr.hr_manager"] },
                { "fields": ["notes"], "read_roles": ["hr.hr_manager"] }
            ]
        }))
    }

    #[test]
    fn a_member_sees_only_their_own_records() -> Result<(), QueryError> {
        let set = leave();
        let me = vars(&[("hr.employee", json!("e:1")), ("hr.subordinates", json!(["e:2", "e:3"]))]);
        let decision = set.decide("hr_leave", Operation::Read, &[], &me)?;
        assert_eq!(decision, Decision::Only(Filter::Cmp { field: "employee".into(), op: Op::Eq, value: json!("e:1") }));
        Ok(())
    }

    #[test]
    fn roles_widen_what_is_visible() -> Result<(), QueryError> {
        let set = leave();
        let me = vars(&[("hr.employee", json!("e:1")), ("hr.subordinates", json!(["e:2", "e:3"]))]);
        let Decision::Only(Filter::Or(parts)) = set.decide("hr_leave", Operation::Read, &roles(&["hr_leave.approver"]), &me)? else {
            return Err(QueryError::Invalid("expected an or of grants".into()));
        };
        assert_eq!(parts.len(), 2, "own, plus their team");
        // The HR manager's grant has no `when`: everything.
        assert_eq!(set.decide("hr_leave", Operation::Read, &roles(&["hr.hr_manager"]), &me)?, Decision::Open);
        Ok(())
    }

    #[test]
    fn restrictions_narrow_and_exemptions_lift_them() -> Result<(), QueryError> {
        let set = leave();
        let me = vars(&[("hr.employee", json!("e:1"))]);
        // Writing: own records, and only while submitted.
        let Decision::Only(filter) = set.decide("hr_leave", Operation::Write, &[], &me)? else {
            return Err(QueryError::Invalid("expected a filter".into()));
        };
        assert!(filter.fields().contains(&"state"));
        assert!(filter.fields().contains(&"employee"));
        // The HR manager is exempt from the restriction and has an unconditional grant.
        assert_eq!(set.decide("hr_leave", Operation::Write, &roles(&["hr.hr_manager"]), &me)?, Decision::Open);
        Ok(())
    }

    #[test]
    fn a_variable_without_a_value_matches_nothing() -> Result<(), QueryError> {
        let set = leave();
        // Not an employee: no `hr.employee`, and nothing else grants.
        assert_eq!(set.decide("hr_leave", Operation::Read, &[], &HashMap::new())?, Decision::Deny);
        assert_eq!(
            set.decide("hr_leave", Operation::Read, &[], &vars(&[("hr.employee", Value::Null)]))?,
            Decision::Deny
        );
        // An empty team grants nothing from that row, but own still does.
        let me = vars(&[("hr.employee", json!("e:1")), ("hr.subordinates", json!([]))]);
        let decision = set.decide("hr_leave", Operation::Read, &roles(&["hr_leave.approver"]), &me)?;
        assert!(matches!(decision, Decision::Only(_)));
        Ok(())
    }

    #[test]
    fn an_operation_no_grant_names_is_open_and_one_a_grant_names_is_closed_to_others() -> Result<(), QueryError> {
        let set = rules(json!({ "model": "m", "access": [ { "name": "a", "roles": ["boss"], "operations": ["delete"] } ] }));
        assert_eq!(set.decide("p", Operation::Read, &[], &HashMap::new())?, Decision::Open, "nothing governs reading");
        assert_eq!(set.decide("p", Operation::Delete, &[], &HashMap::new())?, Decision::Deny);
        assert_eq!(set.decide("p", Operation::Delete, &roles(&["p.boss"]), &HashMap::new())?, Decision::Open);
        assert!(set.limits(Operation::Delete) && !set.limits(Operation::Read));
        Ok(())
    }

    #[test]
    fn needed_variables_are_only_those_of_rules_that_apply() {
        let set = leave();
        let plain = set.variables_needed("hr_leave", Operation::Read, &[]);
        assert_eq!(plain.into_iter().collect::<Vec<_>>(), ["hr.employee"]);
        let approver = set.variables_needed("hr_leave", Operation::Read, &roles(&["hr_leave.approver"]));
        assert_eq!(approver.into_iter().collect::<Vec<_>>(), ["hr.employee", "hr.subordinates"]);
        assert!(set.variables_needed("hr_leave", Operation::Read, &roles(&["hr.hr_manager"])).len() == 1);
    }

    #[test]
    fn field_rules_hide_and_lock_for_those_without_the_role() {
        let set = leave();
        assert!(set.hidden_fields("hr_leave", &[]).contains("notes"));
        assert!(set.hidden_fields("hr_leave", &roles(&["hr.hr_manager"])).is_empty());
        assert!(set.locked_fields("hr_leave", &[]).contains("state"));
        assert!(set.locked_fields("hr_leave", &roles(&["hr_leave.approver"])).is_empty());
        assert!(!set.hidden_fields("hr_leave", &[]).contains("state"), "only reading is limited for notes");
    }

    #[test]
    fn literal_dollar_text_is_kept() -> Result<(), QueryError> {
        let set = rules(json!({ "model": "m", "restrict": [ { "name": "r", "when": { "tag": "$$cash" } } ] }));
        let Decision::Only(Filter::Cmp { value, .. }) = set.decide("p", Operation::Read, &[], &HashMap::new())? else {
            return Err(QueryError::Invalid("expected a filter".into()));
        };
        assert_eq!(value, json!("$cash"));
        Ok(())
    }

    #[test]
    fn rules_are_checked_against_the_model() -> Result<(), serde_json::Error> {
        let mut model: ModelDef = serde_json::from_value(json!({
            "name": "m", "fields": [ { "name": "owner", "type": "string" }, { "name": "state", "type": "string" } ]
        }))?;
        super::super::definition::sync_ids(&mut model);
        let good = rules(json!({ "model": "m", "access": [ { "name": "own", "operations": ["read"], "when": { "owner": "$user" } } ],
                                  "fields": [ { "fields": ["state"], "write_roles": ["boss"] } ] }));
        assert!(good.problems(&model).is_empty(), "{:?}", good.problems(&model));
        let bad = rules(json!({ "model": "m", "access": [
                { "name": "x", "operations": [], "when": { "nope": 1 } },
                { "name": "x", "operations": ["read"], "when": { "owner": { "bogus": 1 } } } ],
                "fields": [ { "fields": ["ghost"] }, { "fields": [], "read_roles": [] } ] }));
        let problems = bad.problems(&model);
        assert!(problems.iter().any(|p| p.contains("two rules")));
        assert!(problems.iter().any(|p| p.contains("no operations")));
        assert!(problems.iter().any(|p| p.contains("`nope`")));
        assert!(problems.iter().any(|p| p.contains("unknown operator") || p.contains("invalid filter")));
        assert!(problems.iter().any(|p| p.contains("ghost")));
        assert!(problems.iter().any(|p| p.contains("limits neither")));
        Ok(())
    }

    #[test]
    fn an_unknown_key_in_a_rule_file_is_an_error() {
        assert!(RuleSet::parse(r#"{ "model": "m", "acess": [] }"#).is_err());
        assert!(RuleSet::parse(r#"{ "model": "m", "access": [ { "name": "a", "operations": ["peek"] } ] }"#).is_err());
    }

    #[test]
    fn qualifying_roles() {
        assert_eq!(qualify("hr_leave", "approver"), "hr_leave.approver");
        assert_eq!(qualify("hr_leave", "hr.hr_manager"), "hr.hr_manager");
        assert_eq!(qualify("hr_leave", "org_admin"), "org_admin");
        assert_eq!(qualify("hr", "via:*"), "via:*");
        assert_eq!(qualify("hr", "via:hr_onboarding"), "via:hr_onboarding");
    }
}
