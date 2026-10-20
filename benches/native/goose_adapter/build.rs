use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let manifest = manifest_dir.join("../goose_sources.json");
    let pins: serde_json::Value = serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    let source = PathBuf::from(
        env::var_os("GOOSE_NATIVE_SOURCE_DIR")
            .expect("run benches/native/goose_adapter.py to acquire the pinned official source"),
    );
    for entry in pins["files"].as_array().unwrap() {
        let name = entry["path"].as_str().unwrap();
        let path = source.join(name);
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let actual = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(
            actual,
            entry["sha256"].as_str().unwrap(),
            "untrusted Goose source: {name}"
        );
        println!("cargo:rerun-if-changed={}", path.display());
    }
    println!("cargo:rerun-if-env-changed=GOOSE_NATIVE_SOURCE_DIR");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!(
        "cargo:rustc-env=GOOSE_NATIVE_EDIT_SOURCE={}",
        source
            .join("crates/goose/src/agents/platform_extensions/developer/edit.rs")
            .display()
    );
    println!(
        "cargo:rustc-env=GOOSE_NATIVE_MCP_SOURCE={}",
        source
            .join("crates/goose/src/agents/mcp_client.rs")
            .display()
    );
    let registration = fs::read_to_string(
        source.join("crates/goose/src/agents/platform_extensions/developer/mod.rs"),
    )
    .unwrap();
    // Compile the original first two Tool declarations, including annotations,
    // rather than inventing a cheaper metadata shape for this benchmark.
    let schema = registration
        .split_once("    fn schema<T: JsonSchema>() -> JsonObject {")
        .unwrap()
        .1
        .split_once("    pub fn parse_args")
        .unwrap()
        .0;
    let declarations = registration
        .split_once("    pub(crate) fn get_tools() -> Vec<Tool> {")
        .unwrap()
        .1
        .split_once("vec![")
        .unwrap()
        .1
        .split_once("            {\n                let shell =")
        .unwrap()
        .0;
    let generated = format!("use rmcp::model::{{JsonObject, Tool, ToolAnnotations}};\nuse schemars::{{schema_for, JsonSchema}};\nuse crate::official_edit::{{FileWriteParams, FileEditParams}};\npub struct OfficialCatalog;\nimpl OfficialCatalog {{\nfn schema<T: JsonSchema>() -> JsonObject {{{schema}\npub fn edit_tools() -> Vec<Tool> {{ vec![{declarations}] }}\n}}\n");
    fs::write(
        PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("goose_catalog.rs"),
        generated,
    )
    .unwrap();
}
