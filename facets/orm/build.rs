//! Migrations are embedded with `include_dir!`, which Cargo does not track:
//! adding a migration file would otherwise leave a stale build. Watching the
//! directory makes a new file trigger a rebuild.

fn main() {
    println!("cargo:rerun-if-changed=../../migrations");
}
