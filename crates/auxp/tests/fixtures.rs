//! The crate against the contract: every valid fixture passes, every
//! invalid one fails, and the schema lists the same methods and required
//! params as [`auxp::methods`].

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::Value;

fn read(path: &str) -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(root.join(path)).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap()
}

#[test]
fn valid_fixtures_pass_and_invalid_ones_fail() {
    let fixtures = read("fixtures/v1/messages.json");
    for message in fixtures["valid"].as_array().unwrap() {
        if let Err(e) = auxp::validate(message) {
            panic!("valid fixture {} was refused: {e}", message["method"]);
        }
    }
    for message in fixtures["invalid"].as_array().unwrap() {
        assert!(
            auxp::validate(message).is_err(),
            "invalid fixture was accepted: {}",
            message["expectedError"]
        );
    }
}

#[test]
fn the_schema_and_the_crate_list_the_same_methods_and_params() {
    let schema = read("schemas/v1/messages.schema.json");
    assert_eq!(
        schema["$defs"]["baseParams"]["properties"]["protocolVersion"]["const"],
        auxp::PROTOCOL_VERSION
    );
    let base: Vec<&str> = schema["$defs"]["baseParams"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(base, auxp::methods::BASE_PARAMS);

    let mut in_schema = BTreeSet::new();
    for branch in schema["oneOf"].as_array().unwrap() {
        let definition = &branch["allOf"][1]["properties"];
        let methods: Vec<&str> = match definition["method"]["const"].as_str() {
            Some(method) => vec![method],
            None => definition["method"]["enum"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect(),
        };
        let required: BTreeSet<&str> = definition["params"]["required"]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_str().unwrap()).collect())
            .unwrap_or_default();
        for method in methods {
            let ours: BTreeSet<&str> = auxp::methods::required_params(method)
                .unwrap_or_else(|| panic!("the schema has {method}, the crate does not"))
                .iter()
                .copied()
                .collect();
            assert_eq!(ours, required, "required params of {method}");
            in_schema.insert(method);
        }
    }
    let ours: BTreeSet<&str> = auxp::methods::NODE
        .iter()
        .chain(auxp::methods::COMMANDS)
        .chain(auxp::methods::EVENTS)
        .copied()
        .collect();
    assert_eq!(ours, in_schema);
}
