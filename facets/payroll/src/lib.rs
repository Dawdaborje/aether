//! Payroll calculation in the kernel, for plugins: the Swift payroll engine run natively, on every core.
//!
//! A plugin's WebAssembly runs on one thread with a fuel budget, and compiling a large salary structure and
//! evaluating it for each person is most of what a payroll batch costs there. The kernel command
//! `payroll::calculate` hands that part to this crate instead: the structure is compiled once and kept
//! (keyed by the plugin's own name for it, which must change whenever the structure does: a hash), and the
//! people of a call are calculated in parallel with rayon once there is enough work to share out.
//!
//! The answer has the same numbers the engine gives inside the plugin: every amount is the exact decimal, as
//! text. Numbers in a person's variables reach the kernel as JSON numbers (an f64 on the way, exact up to 15
//! significant digits); a plugin that holds a figure with more digits calculates that person itself.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

use rayon::prelude::*;
use serde_json::{Map, Value, json};
pub use sp_engine::Decimal;
use sp_engine::{Compiled, Input, Structure};

/// Most people in one call.
pub const MAX_PEOPLE: usize = 5_000;
/// Compiled structures kept, over all organizations and plugins; the oldest goes first.
const CACHE_ENTRIES: usize = 64;
/// Below this many component evaluations (people × components) a call runs on its own thread: sharing out
/// the work would cost more than it saves.
const PARALLEL_FROM: usize = 4_000;

/// Compiled structures, by scope (organization and plugin) and the plugin's key.
pub struct Cache {
    inner: Mutex<(HashMap<String, Arc<Compiled>>, VecDeque<String>)>,
}

impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}

impl Cache {
    pub fn new() -> Self {
        Self { inner: Mutex::new((HashMap::new(), VecDeque::new())) }
    }

    fn get(&self, key: &str) -> Option<Arc<Compiled>> {
        self.inner.lock().ok()?.0.get(key).cloned()
    }

    fn insert(&self, key: String, compiled: Arc<Compiled>) {
        let Ok(mut inner) = self.inner.lock() else { return };
        let (map, order) = &mut *inner;
        if map.insert(key.clone(), compiled).is_none() {
            order.push_back(key);
        }
        while order.len() > CACHE_ENTRIES {
            if let Some(oldest) = order.pop_front() {
                map.remove(&oldest);
            }
        }
    }

    /// How many structures are kept.
    pub fn len(&self) -> usize {
        self.inner.lock().map(|inner| inner.0.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The kernel's cache.
pub fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(Cache::new)
}

/// Payslip lines a call kept in the kernel (`"lines": "keep"`) for the plugin to write with `payroll::write`, so
/// they never pass through its WebAssembly: by scope and ticket, each person's lines (comma-joined amounts).
pub struct Tickets {
    inner: Mutex<HashMap<String, (std::time::Instant, HashMap<String, String>)>>,
}

/// How long kept lines wait to be written.
const TICKET_SECS: u64 = 300;
/// Tickets kept at once, over all organizations and plugins; the oldest go first.
const TICKETS: usize = 512;

impl Default for Tickets {
    fn default() -> Self {
        Self::new()
    }
}

impl Tickets {
    pub fn new() -> Self {
        Self { inner: Mutex::new(HashMap::new()) }
    }

    /// Keep `lines` (person id → lines) and answer their ticket.
    pub fn keep(&self, scope: &str, lines: HashMap<String, String>) -> String {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        let ticket = format!("pl{:x}{:x}", nanos, NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        let Ok(mut inner) = self.inner.lock() else { return ticket };
        let now = std::time::Instant::now();
        inner.retain(|_, (made, _)| now.duration_since(*made).as_secs() < TICKET_SECS);
        while inner.len() >= TICKETS {
            let Some(oldest) = inner.iter().min_by_key(|(_, (made, _))| *made).map(|(k, _)| k.clone()) else { break };
            inner.remove(&oldest);
        }
        inner.insert(scoped(scope, &ticket), (now, lines));
        ticket
    }

    /// Take the lines of a ticket (they are written once). `None` when it is unknown or has expired.
    pub fn take(&self, scope: &str, ticket: &str) -> Option<HashMap<String, String>> {
        let mut inner = self.inner.lock().ok()?;
        let (made, lines) = inner.remove(&scoped(scope, ticket))?;
        (made.elapsed().as_secs() < TICKET_SECS).then_some(lines)
    }

    /// Put lines back (a write that failed may be tried again with the same ticket).
    pub fn put_back(&self, scope: &str, ticket: &str, lines: HashMap<String, String>) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.insert(scoped(scope, ticket), (std::time::Instant::now(), lines));
        }
    }
}

/// The kernel's kept lines.
pub fn tickets() -> &'static Tickets {
    static TICKETS_: OnceLock<Tickets> = OnceLock::new();
    TICKETS_.get_or_init(Tickets::new)
}

impl Cache {
    /// Whether a structure is kept under this key, for this scope.
    pub fn holds(&self, scope: &str, key: &str) -> bool {
        self.get(&scoped(scope, key)).is_some()
    }
}

/// Add comma-joined amounts (a structure's component sums, as payslips and batches keep them) to `total`,
/// position by position. An empty text adds nothing.
pub fn add_lines(total: &mut Vec<Decimal>, text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    for (i, part) in text.split(',').enumerate() {
        let amount: Decimal = if part.is_empty() { Decimal::ZERO } else { part.parse().map_err(|e| format!("`{part}` is not an amount: {e}"))? };
        if i == total.len() {
            total.push(amount);
        } else {
            total[i] = total[i].checked_add(amount).ok_or_else(|| "a sum is too large".to_string())?;
        }
    }
    Ok(())
}

/// Amounts as comma-joined text, the form [`add_lines`] reads.
pub fn lines_text(amounts: &[Decimal]) -> String {
    amounts.iter().map(ToString::to_string).collect::<Vec<_>>().join(",")
}

/// One person to calculate.
#[derive(Debug, Clone)]
pub struct Person {
    pub id: String,
    /// The key of the structure, as given in the call's `structures`.
    pub structure: String,
    pub variables: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalcError {
    /// These structures are neither kept nor given: call again with their definitions.
    Missing(Vec<String>),
    Invalid(String),
}

impl std::fmt::Display for CalcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CalcError::Missing(keys) => write!(f, "structures not known: {}", keys.join(", ")),
            CalcError::Invalid(message) => f.write_str(message),
        }
    }
}

fn scoped(scope: &str, key: &str) -> String {
    format!("{scope}\u{0}{key}")
}

/// Calculate `people`. `structures` maps a key to a structure's definition, or to `null` for one the
/// kernel may already hold under that key; a definition is compiled and kept unless one is already kept
/// under its key. `scope` keeps one plugin's keys apart from another's.
///
/// Answers `{ "results": [ … ], "sums": { key: [ … ] } }`: one result per person, in order, either
/// `{ id, gross, deductions, net, employer_cost, shortfall, lines }` (`lines`: every component's amount, in
/// the structure's order) or `{ id, error }`; and per structure, the sum of each component over the people
/// calculated with it.
///
/// With `lines_text`, `lines` is one string, the amounts joined by commas: far cheaper for a WebAssembly caller to
/// read and to store than an array with one string per component.
pub fn calculate(cache: &Cache, scope: &str, structures: &Map<String, Value>, people: &[Person], lines_text: bool) -> Result<Value, CalcError> {
    if people.len() > MAX_PEOPLE {
        return Err(CalcError::Invalid(format!("at most {MAX_PEOPLE} people in one call")));
    }
    let mut compiled: HashMap<&str, Arc<Compiled>> = HashMap::new();
    for (key, definition) in structures {
        if key.is_empty() || key.len() > 200 {
            return Err(CalcError::Invalid("a structure key is 1 to 200 bytes".into()));
        }
        if let Some(found) = cache.get(&scoped(scope, key)) {
            compiled.insert(key, found);
            continue;
        }
        if definition.is_null() {
            continue;
        }
        let structure: Structure = serde_json::from_value(definition.clone()).map_err(|e| CalcError::Invalid(format!("structure {key}: {e}")))?;
        let made = Arc::new(Compiled::new(&structure).map_err(|e| CalcError::Invalid(format!("structure {key}: {e}")))?);
        cache.insert(scoped(scope, key), made.clone());
        compiled.insert(key, made);
    }
    let mut missing: Vec<String> = Vec::new();
    for person in people {
        if compiled.contains_key(person.structure.as_str()) || missing.contains(&person.structure) {
            continue;
        }
        if !structures.contains_key(&person.structure) {
            return Err(CalcError::Invalid(format!("{}: structure `{}` is not in `structures`", person.id, person.structure)));
        }
        missing.push(person.structure.clone());
    }
    if !missing.is_empty() {
        return Err(CalcError::Missing(missing));
    }

    let one = |person: &Person| -> (Value, Option<Vec<Decimal>>) {
        let engine = &compiled[person.structure.as_str()];
        let input = Input { employee_id: person.id.clone(), variables: person.variables.clone() };
        match engine.calculate_amounts(&input) {
            Err(error) => (json!({ "id": person.id, "error": error.to_string() }), None),
            Ok(calc) => {
                let s = &calc.summary;
                let lines = if lines_text {
                    Value::String(calc.amounts.iter().map(ToString::to_string).collect::<Vec<_>>().join(","))
                } else {
                    Value::Array(calc.amounts.iter().map(|a| Value::String(a.to_string())).collect())
                };
                (
                    json!({
                        "id": person.id, "gross": s.gross.to_string(), "deductions": s.deductions.to_string(), "net": s.net.to_string(),
                        "employer_cost": s.employer_cost.to_string(), "shortfall": s.shortfall.to_string(), "lines": lines,
                    }),
                    Some(calc.amounts),
                )
            }
        }
    };
    let work: usize = people.iter().map(|p| compiled[p.structure.as_str()].structure().components.len()).sum();
    let done: Vec<(Value, Option<Vec<Decimal>>)> =
        if work >= PARALLEL_FROM && people.len() > 1 { people.par_iter().map(one).collect() } else { people.iter().map(one).collect() };

    let mut sums: BTreeMap<&str, Vec<Decimal>> = BTreeMap::new();
    let mut results = Vec::with_capacity(done.len());
    for (person, (result, amounts)) in people.iter().zip(done) {
        if let Some(amounts) = amounts {
            let total = sums.entry(person.structure.as_str()).or_insert_with(|| vec![Decimal::ZERO; amounts.len()]);
            for (sum, amount) in total.iter_mut().zip(&amounts) {
                *sum += *amount;
            }
        }
        results.push(result);
    }
    let sums: Map<String, Value> =
        sums.into_iter().map(|(key, totals)| (key.to_string(), Value::Array(totals.iter().map(|d| Value::String(d.to_string())).collect()))).collect();
    Ok(json!({ "results": results, "sums": sums }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn structure() -> Value {
        json!({
            "id": "s", "version": "1", "currency": "GHS",
            "inputs": [{ "name": "base" }],
            "components": [
                { "id": "basic", "label": "Basic", "kind": "earning", "formula": "base" },
                { "id": "tax", "label": "Tax", "kind": "deduction", "formula": "basic * 0.1" }
            ]
        })
    }

    fn person(id: &str, base: &str) -> Person {
        let base: Value = serde_json::from_str(base).unwrap();
        Person { id: id.into(), structure: "k1".into(), variables: BTreeMap::from([("base".into(), base)]) }
    }

    #[test]
    fn calculates_and_sums_then_reuses_the_kept_structure() {
        let cache = Cache::new();
        let mut structures = Map::new();
        structures.insert("k1".into(), structure());
        let out = calculate(&cache, "org/plugin", &structures, &[person("a", "1000"), person("b", "250.50")], false).unwrap();
        assert_eq!(out["results"][0]["net"], "900.00");
        assert_eq!(out["results"][1]["lines"], json!(["250.50", "25.05"]));
        assert_eq!(out["sums"]["k1"], json!(["1250.50", "125.05"]));
        // Kept: the definition need not be sent again, by the same scope.
        structures.insert("k1".into(), Value::Null);
        assert!(calculate(&cache, "org/plugin", &structures, &[person("c", "10")], false).is_ok());
        assert_eq!(calculate(&cache, "other/plugin", &structures, &[person("c", "10")], false), Err(CalcError::Missing(vec!["k1".into()])));
    }

    #[test]
    fn a_failing_person_does_not_stop_the_others() {
        let cache = Cache::new();
        let mut structures = Map::new();
        structures.insert("k1".into(), structure());
        let mut bad = person("bad", "1");
        bad.variables.clear();
        let out = calculate(&cache, "x", &structures, &[bad, person("ok", "1")], false).unwrap();
        assert!(out["results"][0]["error"].is_string());
        assert_eq!(out["results"][1]["gross"], "1.00");
        assert_eq!(out["sums"]["k1"], json!(["1.00", "0.10"]));
        let text = calculate(&cache, "x", &structures, &[person("t", "250.50")], true).unwrap();
        assert_eq!(text["results"][0]["lines"], "250.50,25.05");
    }

    #[test]
    fn kept_lines_are_taken_once_and_only_by_their_scope() {
        let tickets = Tickets::new();
        let ticket = tickets.keep("org/p", HashMap::from([("a".to_string(), "1.00,2.00".to_string())]));
        assert!(tickets.take("other/p", &ticket).is_none());
        let lines = tickets.take("org/p", &ticket).unwrap();
        assert_eq!(lines["a"], "1.00,2.00");
        assert!(tickets.take("org/p", &ticket).is_none());
        tickets.put_back("org/p", &ticket, lines);
        assert!(tickets.take("org/p", &ticket).is_some());
        let cache = Cache::new();
        let mut structures = Map::new();
        structures.insert("k1".into(), structure());
        calculate(&cache, "x", &structures, &[], false).unwrap();
        assert!(cache.holds("x", "k1") && !cache.holds("y", "k1"));
    }

    #[test]
    fn lines_add_up_position_by_position() {
        let mut total = Vec::new();
        add_lines(&mut total, "1.10,2.00").unwrap();
        add_lines(&mut total, "0.90,0.50,3.00").unwrap();
        add_lines(&mut total, "").unwrap();
        assert_eq!(lines_text(&total), "2.00,2.50,3.00");
        assert!(add_lines(&mut total, "x").is_err());
    }

    #[test]
    fn parallel_and_serial_agree() {
        let cache = Cache::new();
        let mut structures = Map::new();
        structures.insert("k1".into(), structure());
        let people: Vec<Person> = (0..3000).map(|i| person(&i.to_string(), &format!("{}.{}", 1000 + i, i % 100))).collect();
        let parallel = calculate(&cache, "x", &structures, &people, false).unwrap();
        let serial: Vec<Value> = people.chunks(1).map(|p| calculate(&cache, "x", &structures, p, false).unwrap()["results"][0].clone()).collect();
        assert_eq!(parallel["results"].as_array().unwrap(), &serial);
    }
}
