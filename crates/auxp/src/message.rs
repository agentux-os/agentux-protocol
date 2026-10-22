//! Checking AUXP messages, and the params Command reads from them.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::PROTOCOL_VERSION;
use crate::methods::{self, BASE_PARAMS, Kind};

/// Params no message may carry: provider credentials stay on the node.
const CREDENTIAL_PARAMS: &[&str] = &["apiKey", "providerToken"];

/// Checks `message` against AUXP v1: the JSON-RPC envelope, a known method,
/// the identity every message carries, the params the method requires, no
/// provider credentials, and the shape of what Command relies on (a
/// positive event sequence, the hello's capabilities, the agent lists of
/// `harness.status` and `harness.limits_changed`).
pub fn validate(message: &Value) -> Result<(), String> {
    if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err("jsonrpc must be 2.0".into());
    }
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    if method.is_empty() {
        return Err("method is required".into());
    }
    let Some(params) = message.get("params").and_then(Value::as_object) else {
        return Err("params must be an object".into());
    };
    let text = |name: &str| params.get(name).and_then(Value::as_str).unwrap_or("");
    for name in BASE_PARAMS {
        if text(name).is_empty() {
            return Err(format!("{name} is required"));
        }
    }
    if text("protocolVersion") != PROTOCOL_VERSION {
        return Err(format!(
            "unsupported protocol version {:?}",
            text("protocolVersion")
        ));
    }
    if let Err(e) = text("occurredAt").parse::<jiff::Timestamp>() {
        return Err(format!("occurredAt must be RFC3339: {e}"));
    }
    if CREDENTIAL_PARAMS
        .iter()
        .any(|name| params.contains_key(*name))
    {
        return Err("provider credentials are not protocol payloads".into());
    }
    let Some(kind) = methods::kind(method) else {
        return Err(format!("unsupported AUXP method {method:?}"));
    };
    for name in methods::required_params(method).unwrap_or_default() {
        if params.get(*name).is_none_or(Value::is_null) {
            return Err(format!("{method} requires {name}"));
        }
    }
    let non_empty_list = |name: &str| {
        params
            .get(name)
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty())
    };
    match method {
        "node.hello" => {
            if !non_empty_list("capabilities")
                || text("agentuxVersion").is_empty()
                || !non_empty_list("supportedProtocolVersions")
            {
                return Err("node.hello requires capabilities, agentuxVersion and supportedProtocolVersions".into());
            }
        }
        "node.heartbeat" if text("status").is_empty() => {
            return Err("node.heartbeat requires status and capacity".into());
        }
        "node.resume" => {
            if params
                .get("lastAcknowledgedSequence")
                .and_then(Value::as_i64)
                .is_none_or(|s| s < 0)
            {
                return Err("lastAcknowledgedSequence must be a non-negative integer".into());
            }
        }
        "harness.status" if params.get("harnesses").is_none_or(Value::is_null) => {
            return Err("harness.status requires harnesses".into());
        }
        "harness.limits_changed" if params.get("limits").is_none_or(Value::is_null) => {
            return Err("harness.limits_changed requires limits".into());
        }
        _ => {}
    }
    if kind == Kind::Event
        && (text("eventId").is_empty()
            || params
                .get("sequence")
                .and_then(Value::as_i64)
                .is_none_or(|s| s < 1))
    {
        return Err(format!("{method} requires eventId and a positive sequence"));
    }
    Ok(())
}

/// The params of a node's message, as Command reads them. Fields a message
/// does not carry are empty.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Params {
    pub protocol_version: String,
    pub organization_id: String,
    pub node_id: String,
    pub message_id: String,
    pub occurred_at: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub command_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub event_id: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub sequence: i64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub run_id: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    #[serde(rename = "agentuxVersion", skip_serializing_if = "String::is_empty")]
    pub agentux_version: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub supported_protocol_versions: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<Value>,
    /// The node's agent CLIs (`harness.status`), as it sent them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harnesses: Option<Value>,
    /// The usage limits in force (`harness.limits_changed`), as sent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<Value>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub request_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub state: String,
}

fn is_zero(n: &i64) -> bool {
    *n == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message(method: &str, extra: Value) -> Value {
        let mut params = json!({
            "protocolVersion": "v1", "organizationId": "org-1", "nodeId": "node-1",
            "messageId": "msg-1", "occurredAt": "2026-10-15T12:00:00Z",
        });
        params
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        json!({"jsonrpc": "2.0", "method": method, "params": params})
    }

    #[test]
    fn checks_node_messages() {
        let hello = json!({"capabilities": ["run.start"], "agentuxVersion": "0.5.0", "supportedProtocolVersions": ["v1"]});
        validate(&message("node.hello", hello)).unwrap();
        assert!(validate(&message("node.hello", json!({"capabilities": [], "agentuxVersion": "0.5.0", "supportedProtocolVersions": ["v1"]}))).is_err());
        validate(&message(
            "node.heartbeat",
            json!({"status": "ready", "capacity": {}}),
        ))
        .unwrap();
        assert!(validate(&message("node.heartbeat", json!({"status": "ready"}))).is_err());
        validate(&message(
            "run.updated",
            json!({"eventId": "e", "sequence": 3}),
        ))
        .unwrap();
        assert!(
            validate(&message(
                "run.updated",
                json!({"eventId": "e", "sequence": 0})
            ))
            .is_err()
        );
        assert!(
            validate(&message(
                "harness.status",
                json!({"eventId": "e", "sequence": 1})
            ))
            .is_err()
        );
        validate(&message(
            "harness.status",
            json!({"eventId": "e", "sequence": 1, "harnesses": []}),
        ))
        .unwrap();
    }

    #[test]
    fn refuses_what_is_not_auxp() {
        assert!(validate(&message("node.secret", json!({}))).is_err());
        assert!(
            validate(&message(
                "harness.list",
                json!({"commandId": "c", "providerToken": "x"})
            ))
            .is_err()
        );
        assert!(
            validate(&message(
                "run.updated",
                json!({"eventId": "e", "sequence": 1, "occurredAt": "yesterday"})
            ))
            .is_err()
        );
        let mut old = message("node.heartbeat", json!({"status": "ready", "capacity": {}}));
        old["params"]["protocolVersion"] = json!("v0");
        assert!(validate(&old).is_err());
        assert!(
            validate(&json!({"jsonrpc": "1.0", "method": "node.hello", "params": {}})).is_err()
        );
    }

    #[test]
    fn params_read_what_command_needs() {
        let m = message(
            "harness.limits_changed",
            json!({"eventId": "e", "sequence": 2, "limits": [{"kind": "weekly"}]}),
        );
        let params: Params = serde_json::from_value(m["params"].clone()).unwrap();
        assert_eq!(params.node_id, "node-1");
        assert_eq!(params.sequence, 2);
        assert_eq!(params.limits, Some(json!([{"kind": "weekly"}])));
    }
}
