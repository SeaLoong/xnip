//! Keep executable manifest examples in the public documentation parseable.
fn json_blocks(source: &str) -> impl Iterator<Item = &str> {
    source
        .split("```json")
        .skip(1)
        .map(|part| part.split("```").next().unwrap().trim())
}

#[test]
fn documented_json_manifests_parse() {
    for source in [
        include_str!("../docs/apply-format.md"),
        include_str!("../docs/examples.md"),
        include_str!("../docs/cli.md"),
    ] {
        let mut examples = 0;
        for block in json_blocks(source) {
            let value: serde_json::Value = serde_json::from_str(block).unwrap();
            if value.is_array() {
                xnip::apply::parse_json::parse(block).unwrap();
                examples += 1;
            }
        }
        assert!(examples > 0);
    }
}

#[test]
fn documented_mcp_inline_manifest_parses() {
    let source = include_str!("../docs/mcp.md");
    let mut found = false;
    for block in json_blocks(source) {
        let value: serde_json::Value = serde_json::from_str(block).unwrap();
        if let Some(manifest) = value["arguments"]["manifest_text"].as_str() {
            let ops = xnip::apply::parse_native::parse(manifest).unwrap();
            assert_eq!(ops.len(), 2);
            found = true;
        }
    }
    assert!(found);
}
