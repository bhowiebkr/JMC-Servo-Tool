//! Builds the help pages in `docs/parameters` and `docs/topics` into the library.

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("docs");
    let mut out = String::from("pub(crate) static RAW_PAGES: &[(&str, &str)] = &[\n");
    for (dir, prefix) in [("parameters", ""), ("topics", "topic:")] {
        let dir = root.join(dir);
        println!("cargo:rerun-if-changed={}", dir.display());
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut files: Vec<_> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect();
        files.sort();
        for path in files {
            let stem = path.file_stem().and_then(|s| s.to_str()).expect("page file names are UTF-8");
            if stem.eq_ignore_ascii_case("readme") {
                continue;
            }
            writeln!(out, "    ({:?}, include_str!({:?})),", format!("{prefix}{stem}"), path.display().to_string()).unwrap();
        }
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("pages.rs");
    std::fs::write(dest, out).expect("write pages.rs");
}
