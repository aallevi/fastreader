//! Books live on the SD card (and in the books partition for uploads without
//! a card); none are built in any more. Only the short welcome text in
//! ../books/sample.md is embedded, shown when there is no book at all.
use std::{env, fs, path::Path};

fn main() {
    let sample = fs::canonicalize("../books/sample.md").unwrap();
    println!("cargo:rerun-if-changed={}", sample.display());
    let out = format!(
        "/// No built-in library: books come from the SD card and uploads.\npub static BOOKS: &[(&str, &[u8])] = &[];\n/// Shown when there is no book at all.\npub static WELCOME: &[u8] = include_bytes!({:?});\n",
        sample.display().to_string()
    );
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("books.rs"), out).unwrap();
}
