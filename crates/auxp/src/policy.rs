//! What a node does with each command unless its own policy
//! (`/etc/agentux/node-policy.yaml`) says otherwise.

use serde::{Deserialize, Serialize};

/// A node policy's answer to a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    /// Runs.
    Allow,
    /// Waits for someone at the node to accept or reject it.
    Confirm,
    /// Refused.
    Deny,
}

/// The default decision for every command: what does not change code or
/// settings runs; starting runs and changing preferences wait for someone at
/// the node. Commands not listed are denied.
pub const DEFAULTS: &[(&str, Decision)] = &[
    ("run.snapshot", Decision::Allow),
    ("harness.list", Decision::Allow),
    ("harness.test", Decision::Allow),
    ("run.cancel", Decision::Allow),
    ("run.pause", Decision::Allow),
    ("run.resume", Decision::Allow),
    ("request.resolve", Decision::Allow),
    ("bus.post", Decision::Allow),
    ("limits.clear", Decision::Allow),
    ("run.start", Decision::Confirm),
    ("preferences.set", Decision::Confirm),
];

/// The default decision for `method`.
pub fn default_decision(method: &str) -> Decision {
    DEFAULTS
        .iter()
        .find(|(m, _)| *m == method)
        .map(|(_, d)| *d)
        .unwrap_or(Decision::Deny)
}

/// Why a node held or refused a command, in its `command.denied` event.
pub const REASON_AWAITING_CONFIRMATION: &str = "awaiting_local_confirmation";
pub const REASON_DENIED_BY_POLICY: &str = "denied_by_policy";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::methods;

    #[test]
    fn every_command_has_a_default_and_nothing_else_does() {
        assert_eq!(DEFAULTS.len(), methods::COMMANDS.len());
        for (method, _) in DEFAULTS {
            assert!(methods::is_command(method), "{method}");
        }
        assert_eq!(default_decision("run.start"), Decision::Confirm);
        assert_eq!(default_decision("shell.exec"), Decision::Deny);
    }
}
