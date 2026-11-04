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
fn replay_result_fixtures_enforce_cursor_recovery_invariants() {
    use auxp::replay::ReplayResult;
    let fixtures = read("fixtures/v1/replay.json");
    for result in fixtures["valid"].as_array().unwrap() {
        let parsed: ReplayResult = serde_json::from_value(result.clone()).unwrap();
        parsed
            .validate()
            .unwrap_or_else(|e| panic!("valid replay result refused: {e}"));
    }
    for result in fixtures["invalid"].as_array().unwrap() {
        let mut value = result.clone();
        value.as_object_mut().unwrap().remove("expectedError");
        let parsed = serde_json::from_value::<ReplayResult>(value);
        assert!(
            parsed.is_err() || parsed.unwrap().validate().is_err(),
            "invalid replay fixture accepted: {}",
            result["expectedError"]
        );
    }
}

#[test]
fn node_snapshot_fixtures_enforce_complete_checkpoint_pages_and_ack_safety() {
    let fixtures = read("fixtures/v1/node-snapshot.json");
    for request in fixtures["requests"]["valid"].as_array().unwrap() {
        auxp::validate(request).unwrap();
    }
    for request in fixtures["requests"]["invalid"].as_array().unwrap() {
        assert!(
            auxp::validate(request).is_err(),
            "{}",
            request["expectedError"]
        );
    }
    for request in fixtures["checkpointAck"]["requests"]["valid"]
        .as_array()
        .unwrap()
    {
        auxp::validate(request).unwrap();
    }
    for request in fixtures["checkpointAck"]["requests"]["invalid"]
        .as_array()
        .unwrap()
    {
        assert!(
            auxp::validate(request).is_err(),
            "{}",
            request["expectedError"]
        );
    }

    let mut first_valid_chain = None;
    for pages in fixtures["pages"]["valid"].as_array().unwrap() {
        let encoded: Vec<Vec<u8>> = pages
            .as_array()
            .unwrap()
            .iter()
            .map(serde_json::to_vec)
            .collect::<Result<_, _>>()
            .unwrap();
        let refs: Vec<&[u8]> = encoded.iter().map(Vec::as_slice).collect();
        let validated = auxp::snapshot::validate_page_chain(&refs).unwrap_or_else(|error| {
            panic!("valid checkpoint page chain refused: {error}: {pages}")
        });
        if first_valid_chain.is_none() {
            first_valid_chain = Some(validated);
        }
    }
    for invalid in fixtures["pages"]["invalid"].as_array().unwrap() {
        let encoded: Vec<Vec<u8>> = invalid["pages"]
            .as_array()
            .unwrap()
            .iter()
            .map(serde_json::to_vec)
            .collect::<Result<_, _>>()
            .unwrap();
        let refs: Vec<&[u8]> = encoded.iter().map(Vec::as_slice).collect();
        assert!(
            auxp::snapshot::validate_page_chain(&refs).is_err(),
            "{}",
            invalid["expectedError"]
        );
    }

    let valid_ack = &fixtures["checkpointAck"]["valid"];
    assert_eq!(valid_ack["throughSequence"], 57);
    assert_eq!(valid_ack["allPagesServed"], true);
    assert_eq!(valid_ack["scopeMatches"], true);
    for ack in fixtures["checkpointAck"]["invalid"].as_array().unwrap() {
        assert!(
            ack["throughSequence"] != ack["highWaterSequence"]
                || ack["allPagesServed"] != true
                || ack["scopeMatches"] != true,
            "{}",
            ack["expectedError"]
        );
    }
    let chain = first_valid_chain.unwrap();
    chain
        .validate_checkpoint_ack("org-a", "node-a", "checkpoint-7", 57, 40)
        .unwrap();
    assert!(
        chain
            .validate_checkpoint_ack("org-other", "node-a", "checkpoint-7", 57, 40)
            .is_err()
    );
    assert!(
        chain
            .validate_checkpoint_ack("org-a", "node-a", "wrong-checkpoint", 57, 40)
            .is_err()
    );
    assert!(
        chain
            .validate_checkpoint_ack("org-a", "node-a", "checkpoint-7", 58, 40)
            .is_err()
    );
    assert!(
        chain
            .validate_checkpoint_ack("org-a", "node-a", "checkpoint-7", 57, 39)
            .is_err()
    );

    let empty_page = serde_json::to_vec(&fixtures["pages"]["valid"][1][0]).unwrap();
    let mut oversized_page = empty_page;
    oversized_page.resize(auxp::snapshot::MAX_PAGE_BYTES + 1, b' ');
    assert!(auxp::snapshot::validate_page_chain(&[&oversized_page]).is_err());

    let mut max_sized_page = serde_json::to_vec(&fixtures["pages"]["valid"][1][0]).unwrap();
    max_sized_page.resize(auxp::snapshot::MAX_PAGE_BYTES, b' ');
    let over_total: Vec<&[u8]> = vec![max_sized_page.as_slice(); 65];
    assert!(auxp::snapshot::validate_page_chain(&over_total).is_err());
}

#[test]
fn every_activity_variant_has_a_valid_fixture_and_a_rust_type() {
    let schema = read("schemas/v1/execution-activity.schema.json");
    let declared: BTreeSet<&str> = schema["$defs"]["activity"]["properties"]["type"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    let fixtures = read("fixtures/v1/messages.json");
    let covered: BTreeSet<&str> = fixtures["valid"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["method"] == "execution.activity")
        .map(|message| message["params"]["activity"]["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        covered, declared,
        "each declared variant must have a valid, Rust-validated fixture"
    );
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

/// Parses and checks one fixture.
type Check = fn(&Value) -> Result<(), String>;

fn each<'a>(fixtures: &'a Value, group: &str, kind: &str) -> &'a Vec<Value> {
    fixtures[group][kind]
        .as_array()
        .unwrap_or_else(|| panic!("{group}.{kind}"))
}

/// Parses `value` as `T` and runs its check.
fn accepted<T: serde::de::DeserializeOwned>(
    value: &Value,
    check: impl Fn(&T) -> Result<(), String>,
) -> Result<(), String> {
    let mut value = value.clone();
    if let Some(object) = value.as_object_mut() {
        object.remove("expectedError");
    }
    let parsed: T = serde_json::from_value(value).map_err(|e| e.to_string())?;
    check(&parsed)
}

#[test]
fn enrollment_fixtures() {
    use auxp::enrollment::{Enrolled, Instance, Redemption, Refused};
    let fixtures = read("fixtures/v1/enrollment.json");
    let cases: [(&str, Check); 4] = [
        ("instance", |v| accepted::<Instance>(v, Instance::check)),
        ("redemption", |v| {
            accepted::<Redemption>(v, Redemption::check)
        }),
        ("enrolled", |v| accepted::<Enrolled>(v, Enrolled::check)),
        // The schema's `not: required nodeSecret`: a refusal never carries a credential.
        ("refused", |v| match v.get("nodeSecret") {
            Some(_) => Err("a refusal carries a credential".into()),
            None => accepted::<Refused>(v, |_| Ok(())),
        }),
    ];
    for (group, check) in cases {
        for value in each(&fixtures, group, "valid") {
            if let Err(e) = check(value) {
                panic!("valid {group} fixture refused: {e}: {value}");
            }
        }
        for value in each(&fixtures, group, "invalid") {
            assert!(
                check(value).is_err(),
                "invalid {group} fixture accepted: {}",
                value["expectedError"]
            );
        }
    }
}

#[test]
fn enrollment_shapes_match_the_schema() {
    let schema = read("schemas/v1/enrollment.schema.json");
    let pattern = |name: &str| {
        schema["$defs"][name]["pattern"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(pattern("instanceId"), "^cmd-[0-9a-f]{16}$");
    assert_eq!(
        pattern("enrollmentToken"),
        "^auxe1_cmd-[0-9a-f]{16}_[0-9a-f]{64}$"
    );
    assert_eq!(pattern("nodeCredential"), "^auxn1_[0-9a-f]{64}$");
    let token = auxp::enrollment::token("cmd-0123456789abcdef", &"a".repeat(64));
    assert!(auxp::enrollment::token_instance(&token).is_some());
    assert!(auxp::enrollment::valid_credential(
        &auxp::enrollment::credential(&"b".repeat(64))
    ));
    assert!(!auxp::enrollment::valid_node_id("Build 01"));
}
