//! Ensure the directory `rust-embed` reads from at compile time exists, even
//! when the frontend has never been built in this checkout. Without this, a
//! cold `cargo build` of `serverbee-server` from a fresh clone fails because
//! `#[folder = "../../apps/web/dist/builtin-widgets"]` expects the path to
//! resolve. The empty dir is enough: the runtime registrar logs a friendly
//! warning when `manifest.json` is missing.
fn main() {
    let dir = std::path::Path::new("../../apps/web/dist/builtin-widgets");
    let _ = std::fs::create_dir_all(dir);
    println!("cargo:rerun-if-changed=../../apps/web/dist/builtin-widgets");
}
