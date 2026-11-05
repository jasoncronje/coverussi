//! Embeds the web app: every `.gz` file under portal/dist-board (made by
//! `npm run build` in portal/) is served at its path without the `.gz`.

use std::fmt::Write;
use std::path::{Path, PathBuf};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            files(&p, out);
        } else if p.extension().is_some_and(|x| x == "gz") {
            out.push(p);
        }
    }
}

fn kind(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("json") | Some("webmanifest") => "application/json",
        _ => "application/octet-stream",
    }
}

fn main() {
    embuild::espidf::sysenv::output();

    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../portal/dist-board");
    println!("cargo:rerun-if-changed={}", dist.display());
    let mut found = Vec::new();
    files(&dist, &mut found);
    found.sort();
    let mut code = String::from("pub const ASSETS: &[(&str, &str, &[u8])] = &[\n");
    for f in &found {
        println!("cargo:rerun-if-changed={}", f.display());
        let rel = f.strip_prefix(&dist).unwrap().to_string_lossy().replace('\\', "/");
        let path = format!("/{}", rel.trim_end_matches(".gz"));
        writeln!(code, "    ({path:?}, {:?}, include_bytes!({:?})),", kind(&path), f.display().to_string()).unwrap();
    }
    if found.is_empty() {
        println!("cargo:warning=portal/dist-board is empty: run `npm run build` in portal/ for the web app");
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("assets.rs");
    std::fs::write(out, code).unwrap();
}
