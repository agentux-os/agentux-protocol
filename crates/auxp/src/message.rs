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
    if let Some(isolation) = params.get("agentIsolation") {
        validate_agent_isolation(method, isolation)?;
    }
    if CREDENTIAL_PARAMS
        .iter()
        .any(|name| params.contains_key(*name))
    {
        return Err("provider credentials are not protocol payloads".into());
    }
    if params
        .get("activity")
        .is_some_and(contains_forbidden_activity_key)
    {
        return Err(
            "private reasoning, credentials and unrestricted tool input are not activity payloads"
                .into(),
        );
    }
    let Some(kind) = methods::kind(method) else {
        return Err(format!("unsupported AUXP method {method:?}"));
    };
    for name in methods::required_params(method).unwrap_or_default() {
        if params.get(*name).is_none_or(Value::is_null) {
            return Err(format!("{method} requires {name}"));
        }
    }
    if method == "execution.activity" {
        let activity: crate::activity::Activity =
            serde_json::from_value(params["activity"].clone())
                .map_err(|e| format!("invalid execution activity: {e}"))?;
        activity.validate()?;
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
            let speaks_v1 =
                params["supportedProtocolVersions"]
                    .as_array()
                    .is_some_and(|versions| {
                        versions
                            .iter()
                            .any(|v| v.as_str() == Some(PROTOCOL_VERSION))
                    });
            if !speaks_v1 {
                return Err(format!(
                    "node.hello: the node does not support {PROTOCOL_VERSION}"
                ));
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
        "node.events.ack" => {
            if params
                .get("throughSequence")
                .and_then(Value::as_i64)
                .is_none_or(|s| s < 0)
            {
                return Err("throughSequence must be a non-negative integer".into());
            }
        }
        "node.snapshot" => {
            if params
                .get("requestedAfterSequence")
                .and_then(Value::as_i64)
                .is_none_or(|s| s < 0)
            {
                return Err("requestedAfterSequence must be a non-negative integer".into());
            }
            if params
                .get("pageSize")
                .and_then(Value::as_i64)
                .is_none_or(|s| !(1..=100).contains(&s))
            {
                return Err("pageSize must be between 1 and 100".into());
            }
            if text("checkpointId").chars().count() > 128 {
                return Err("checkpointId must be at most 128 characters".into());
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

fn validate_agent_isolation(method: &str, value: &Value) -> Result<(), String> {
    if !matches!(method, "run.updated" | "run.completed" | "run.failed") {
        return Err(
            "agentIsolation is only valid on run.updated, run.completed and run.failed".into(),
        );
    }
    let Some(object) = value.as_object() else {
        return Err("agentIsolation must be an object".into());
    };
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "mode" | "network" | "source"))
    {
        return Err("agentIsolation contains an unknown field".into());
    }
    let mode = object.get("mode").and_then(Value::as_str).unwrap_or("");
    if !matches!(mode, "none" | "bubblewrap") {
        return Err("agentIsolation.mode must be none or bubblewrap".into());
    }
    let source = object.get("source").and_then(Value::as_str).unwrap_or("");
    if !matches!(source, "project" | "node-policy" | "command" | "default") {
        return Err("agentIsolation.source is invalid".into());
    }
    match (mode, object.get("network")) {
        ("bubblewrap", Some(Value::String(network)))
            if matches!(network.as_str(), "outbound" | "none") => {}
        ("bubblewrap", _) => {
            return Err("bubblewrap agentIsolation requires a valid network mode".into());
        }
        ("none", None) => {}
        ("none", Some(_)) => return Err("host agentIsolation must not include network".into()),
        _ => unreachable!("mode was validated above"),
    }
    Ok(())
}

fn contains_forbidden_activity_key(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, child)| {
            matches!(
                key.as_str(),
                "chainOfThought"
                    | "chain_of_thought"
                    | "privateChainOfThought"
                    | "reasoning"
                    | "privateReasoning"
                    | "apiKey"
                    | "providerToken"
                    | "credentials"
                    | "toolInput"
                    | "rawToolInput"
                    | "rawInput"
                    | "toolOutput"
            ) || contains_forbidden_activity_key(child)
        }),
        Value::Array(items) => items.iter().any(contains_forbidden_activity_key),
        _ => false,
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_acknowledged_sequence: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub through_sequence: Option<i64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub checkpoint_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_after_sequence: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<i64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub page_token: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub run_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_isolation: Option<AgentIsolation>,
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

/// Effective agent-session isolation reported after a run opens a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentIsolation {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    pub source: String,
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
        assert!(
            validate(&message(
                "node.hello",
                json!({"capabilities": ["run.start"], "agentuxVersion": "9.0.0", "supportedProtocolVersions": ["v2"]})
            ))
            .is_err(),
            "a node without v1 is refused"
        );
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
        let sandboxed = message(
            "run.updated",
            json!({"eventId": "e", "sequence": 3, "agentIsolation": {"mode": "bubblewrap", "network": "outbound", "source": "command"}}),
        );
        validate(&sandboxed).unwrap();
        let parsed: Params = serde_json::from_value(sandboxed["params"].clone()).unwrap();
        assert_eq!(parsed.agent_isolation.unwrap().mode, "bubblewrap");
        validate(&message(
            "run.failed",
            json!({"eventId": "e", "sequence": 4, "agentIsolation": {"mode": "none", "source": "default"}}),
        ))
        .unwrap();
        for invalid_isolation in [
            json!(null),
            json!({"mode": "bubblewrap", "source": "command"}),
            json!({"mode": "none", "network": "outbound", "source": "default"}),
            json!({"mode": "none", "source": "unknown"}),
        ] {
            assert!(
                validate(&message(
                    "run.updated",
                    json!({"eventId": "e", "sequence": 3, "agentIsolation": invalid_isolation}),
                ))
                .is_err()
            );
        }
        assert!(
            validate(&message(
                "run.start",
                json!({"agentIsolation": {"mode": "none", "source": "default"}}),
            ))
            .is_err()
        );
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

    #[test]
    fn resume_and_ack_cursors_are_read_as_signed_protocol_sequences() {
        let resume = message(
            "node.resume",
            json!({"commandId":"resume-1","lastAcknowledgedSequence":12}),
        );
        validate(&resume).unwrap();
        let params: Params = serde_json::from_value(resume["params"].clone()).unwrap();
        assert_eq!(params.last_acknowledged_sequence, Some(12));
        let ack = message(
            "node.events.ack",
            json!({"commandId":"ack-1","throughSequence":12}),
        );
        validate(&ack).unwrap();
        let params: Params = serde_json::from_value(ack["params"].clone()).unwrap();
        assert_eq!(params.through_sequence, Some(12));
        let invalid = message(
            "node.resume",
            json!({"commandId":"resume-2","lastAcknowledgedSequence":-1}),
        );
        assert!(validate(&invalid).is_err());
    }
}
