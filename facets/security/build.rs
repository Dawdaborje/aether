//! The capability catalog is embedded with `include_dir!`, which Cargo does not track:
//! adding or editing a file in `capabilities/` would otherwise leave a stale build.
//! Watching the directory makes a change trigger a rebuild.

fn main() {
    println!("cargo:rerun-if-changed=../../capabilities");
}
