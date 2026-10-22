//! Every AUXP method, who sends it and the params it requires
//! (`schemas/v1/messages.schema.json`).

/// Params every message carries.
pub const BASE_PARAMS: &[&str] = &[
    "protocolVersion",
    "organizationId",
    "nodeId",
    "messageId",
    "occurredAt",
];

/// Node → Command requests.
pub const NODE: &[&str] = &["node.hello", "node.heartbeat", "node.resume"];

/// Command → node commands.
pub const COMMANDS: &[&str] = &[
    "run.start",
    "run.cancel",
    "run.pause",
    "run.resume",
    "run.snapshot",
    "request.resolve",
    "bus.post",
    "harness.list",
    "harness.test",
    "limits.clear",
    "preferences.set",
];

/// Node → Command events: notifications with an `eventId` and a per-node
/// `sequence`.
pub const EVENTS: &[&str] = &[
    "node.status_changed",
    "run.accepted",
    "run.started",
    "run.updated",
    "session.event",
    "approval.requested",
    "gate.completed",
    "run.completed",
    "run.failed",
    "artifact.available",
    "node.alert",
    "harness.status",
    "harness.limits_changed",
    "command.denied",
    "command.completed",
];

/// Who sends a method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A node's request (`node.*`).
    Node,
    /// Command's command to a node.
    Command,
    /// A node's event.
    Event,
}

/// The kind of `method`; `None` for a method AUXP does not have.
pub fn kind(method: &str) -> Option<Kind> {
    if NODE.contains(&method) {
        Some(Kind::Node)
    } else if COMMANDS.contains(&method) {
        Some(Kind::Command)
    } else if EVENTS.contains(&method) {
        Some(Kind::Event)
    } else {
        None
    }
}

pub fn is_command(method: &str) -> bool {
    kind(method) == Some(Kind::Command)
}

pub fn is_event(method: &str) -> bool {
    kind(method) == Some(Kind::Event)
}

/// The params `method` requires besides [`BASE_PARAMS`], as the schema
/// says. `run.start` lists the full execution request of the schema;
/// today's nodes start a run from `repository` and `prompt`.
pub fn required_params(method: &str) -> Option<&'static [&'static str]> {
    Some(match method {
        "node.hello" => &[
            "capabilities",
            "agentuxVersion",
            "supportedProtocolVersions",
        ],
        "node.heartbeat" => &["status", "capacity"],
        "node.resume" => &["lastAcknowledgedSequence"],
        "run.start" => &[
            "commandId",
            "missionId",
            "runId",
            "repository",
            "prompt",
            "pipeline",
            "roles",
            "budget",
            "isolationPolicy",
            "requestedCapabilities",
        ],
        "run.cancel" | "run.pause" | "run.resume" | "run.snapshot" => &["commandId", "runId"],
        "request.resolve" => &["commandId", "runId", "requestId", "resolution"],
        "bus.post" => &["commandId", "runId", "message"],
        "harness.list" => &["commandId"],
        "harness.test" | "limits.clear" => &["commandId", "harness"],
        "preferences.set" => &["commandId", "preferences"],
        event if is_event(event) => &["eventId", "sequence"],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_method_has_one_kind_and_its_required_params() {
        for method in NODE.iter().chain(COMMANDS).chain(EVENTS) {
            assert!(kind(method).is_some(), "{method}");
            assert!(required_params(method).is_some(), "{method}");
        }
        let total = NODE.len() + COMMANDS.len() + EVENTS.len();
        let mut all: Vec<_> = NODE.iter().chain(COMMANDS).chain(EVENTS).collect();
        all.dedup();
        assert_eq!(all.len(), total, "no method is listed twice");
        assert_eq!(kind("shell.exec"), None);
    }
}
