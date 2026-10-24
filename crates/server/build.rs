//! The server runs on Windows' segment heap (`segment-heap.manifest`),
//! embedded in its executable so every allocation in the process takes it,
//! the engine's and std's libraries linked as DLLs as much as its own. The
//! ladder builds separate areas of terrain on many threads at once, each
//! allocating large blocks as it goes, and the default heap takes one lock
//! for every block that size: on it those threads wait in turn.

fn main() {
    let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("cargo's manifest dir")).join("segment-heap.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}", manifest.display());
    }
}
