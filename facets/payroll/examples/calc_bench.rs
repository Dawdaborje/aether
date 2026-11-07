//! What `payroll::calculate` costs inside the kernel, without the database or the plugin around it.
//!
//! `cargo run --release -p aether_payroll --example calc_bench -- <structure.json> <people.ndjson> [batch]`
//! (the swift_payroll_engine repo's `example_data/stress.rs` makes both files).

use std::time::Instant;

use aether_payroll::{Cache, Person, calculate};
use serde_json::{Map, Value};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let definition: Value = serde_json::from_str(&std::fs::read_to_string(&args[1]).unwrap()).unwrap();
    let batch: usize = args.get(3).map_or(100, |b| b.parse().unwrap());
    let people: Vec<Person> = std::fs::read_to_string(&args[2])
        .unwrap()
        .lines()
        .take(batch)
        .map(|line| {
            let row: Value = serde_json::from_str(line).unwrap();
            Person {
                id: row["employee_id"].as_str().unwrap().to_string(),
                structure: "k".into(),
                variables: row["variables"].as_object().unwrap().clone().into_iter().collect(),
            }
        })
        .collect();
    let components = definition["components"].as_array().map_or(0, Vec::len);
    let mut given = Map::new();
    given.insert("k".into(), definition);
    let cache = Cache::new();

    let started = Instant::now();
    calculate(&cache, "bench", &given, &people[..1], false).unwrap();
    println!("{components} components: compile + 1 person      {:>9.2} ms", started.elapsed().as_secs_f64() * 1e3);

    let mut cached = Map::new();
    cached.insert("k".into(), Value::Null);
    let rounds = 20;
    let started = Instant::now();
    for _ in 0..rounds {
        calculate(&cache, "bench", &cached, &people, true).unwrap();
    }
    let per_call = started.elapsed().as_secs_f64() / rounds as f64;
    println!(
        "{components} components: {} people per call (kept)  {:>9.2} ms  = {:>7.1} µs a payslip",
        people.len(),
        per_call * 1e3,
        per_call * 1e6 / people.len() as f64
    );
    let started = Instant::now();
    for _ in 0..rounds {
        for person in &people {
            calculate(&cache, "bench", &cached, std::slice::from_ref(person), true).unwrap();
        }
    }
    let serial = started.elapsed().as_secs_f64() / rounds as f64;
    println!("{components} components: same, one person per call {:>9.2} ms  (one core)", serial * 1e3);
}
