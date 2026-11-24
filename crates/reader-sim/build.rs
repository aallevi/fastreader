//! Device info's version: the build after the number when it isn't a release.
fn main() {
    println!("cargo:rustc-env=FR_BUILD={}", build_label());
}

/// The build after the version number: nothing for a released version (the
/// commit has its tag), else " +<commit>" (and "*" with uncommitted
/// changes), so Device info tells a release from a build made since.
fn build_label() -> String {
    let git = |args: &[&str]| std::process::Command::new("git").args(args).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    for p in ["../../.git/HEAD", "../../.git/refs/tags", "../../.git/index"] {
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
