//! Books live on the SD card (and in the books partition for uploads without
//! a card); none are built in any more. Only the short welcome text in
//! ../books/sample.md is embedded, shown when there is no book at all.
use std::{env, fs, path::Path};

fn main() {
    println!("cargo:rustc-env=FR_BUILD={}", build_label());
    let sample = fs::canonicalize("../books/sample.md").unwrap();
    println!("cargo:rerun-if-changed={}", sample.display());
    let out = format!(
        "/// No built-in library: books come from the SD card and uploads.\npub static BOOKS: &[(&str, &[u8])] = &[];\n/// Shown when there is no book at all.\npub static WELCOME: &[u8] = include_bytes!({:?});\n",
        sample.display().to_string()
    );
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("books.rs"), out).unwrap();
}

/// The build after the version number: nothing for a released version (the
/// commit has its tag), else " +<commit>" (and "*" with uncommitted
/// changes), so Device info tells a release from a build made since.
fn build_label() -> String {
    let git = |args: &[&str]| std::process::Command::new("git").args(args).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    for p in ["../.git/HEAD", "../.git/refs/tags", "../.git/index"] {
        println!("cargo:rerun-if-changed={p}");
    }
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty());
    if git(&["describe", "--tags", "--exact-match"]).is_some() && !dirty {
        return String::new();
    }
    match git(&["rev-parse", "--short", "HEAD"]) {
        Some(h) => format!(" +{h}{}", if dirty { "*" } else { "" }),
        None => String::new(),
    }
}
