//! Compiles the generated tree-sitter parser of `tooling/tree-sitter-kagari` into this binary.

fn main() {
    let src = "../tree-sitter-kagari/src";
    println!("cargo::rerun-if-changed={src}/parser.c");
    println!("cargo::rerun-if-changed={src}/tree_sitter");
    cc::Build::new()
        .std("c11")
        .include(src)
        .file(format!("{src}/parser.c"))
        .warnings(false)
        .compile("tree-sitter-kagari");
}
