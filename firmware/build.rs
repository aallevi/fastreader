//! Embeds every book in ../books and ../books/private (converted with
//! tools/convert.py) into flash, as a `BOOKS: &[(&str, &str)]` table.
use std::{env, fs, path::Path};

fn main() {
    let mut books = Vec::new();
    for dir in ["../books", "../books/private"] {
        println!("cargo:rerun-if-changed={dir}");
        let mut found: Vec<_> = fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "md" || e == "txt")).collect();
        found.sort();
        for p in found {
            println!("cargo:rerun-if-changed={}", p.display());
            let stem = p.file_stem().unwrap().to_string_lossy().replace(['-', '_'], " ");
            books.push((stem, fs::canonicalize(&p).unwrap()));
        }
    }
    let mut out = String::from("/// (fallback title, text) for every embedded book.\npub static BOOKS: &[(&str, &str)] = &[\n");
    for (stem, path) in &books {
        out += &format!("    ({stem:?}, include_str!({:?})),\n", path.display().to_string());
    }
    out += "];\n";
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("books.rs"), out).unwrap();
}
