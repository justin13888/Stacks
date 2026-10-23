//! Generate the Stacks client from the committed OpenAPI document.
//!
//! spargen is a second, independent consumer of `web/openapi.json` (orval is
//! the first): if the document stops describing what the server does, or
//! uses a construct a strict 3.1 generator rejects, this crate stops
//! compiling.

fn main() {
    let spec = concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/openapi.json");
    println!("cargo::rerun-if-changed={spec}");
    let out = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    spargen::generate(&spargen::Spec::new(spec).build(format!("{out}/stacks.rs"))).expect_success();
}
