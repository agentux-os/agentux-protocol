//! Typed node status and alert state carried by sequenced AUXP events.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Operational state of an AgentUX node. Fleet presence is derived from
/// heartbeat freshness and is intentionally not an operational state here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Ready,
    Draining,
    Error,
}

/// Complete current node state published by `node.status_changed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NodeState {
    pub node_id: String,
    pub status: NodeStatus,
    pub capacity: Value,
}

impl NodeState {
    pub(crate) fn validate(&self, envelope_node_id: &str) -> Result<(), String> {
        if self.node_id != envelope_node_id {
            return Err("node status nodeId must match the authenticated envelope".into());
        }
        if !self.capacity.is_object() {
            return Err("node status capacity must be an object".into());
        }
        Ok(())
    }
}

/// Lifecycle status of an actionable local node alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertStatus {
    Active,
    Resolved,
}

/// Complete current state for one stable, Core-owned alert identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NodeAlertState {
    pub alert_id: String,
    pub producer: String,
    pub code: String,
    pub condition_key: String,
    pub summary: String,
    pub first_observed_at: String,
    pub last_observed_at: String,
    pub status: AlertStatus,
}

impl NodeAlertState {
    fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("alertId", self.alert_id.as_str()),
            ("producer", self.producer.as_str()),
            ("code", self.code.as_str()),
            ("conditionKey", self.condition_key.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("node alert {name} is required"));
            }
        }
        if self.summary.trim().is_empty() {
            return Err("node alert summary must not be empty".into());
        }
        if self.summary.chars().count() > 2_000 {
            return Err("node alert summary exceeds 2000 Unicode characters".into());
        }
        let first: jiff::Timestamp = self
            .first_observed_at
            .parse()
            .map_err(|error| format!("node alert firstObservedAt must be RFC3339: {error}"))?;
        let last: jiff::Timestamp = self
            .last_observed_at
            .parse()
            .map_err(|error| format!("node alert lastObservedAt must be RFC3339: {error}"))?;
        if last < first {
            return Err("node alert lastObservedAt must not precede firstObservedAt".into());
        }
        Ok(())
    }
}

/// Full alert replacement or an explicit removal at the event sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum NodeAlertEvent {
    State { alert: NodeAlertState },
    Removed { alert_id: String },
}

impl NodeAlertEvent {
    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            Self::State { alert } => alert.validate(),
            Self::Removed { alert_id } if alert_id.trim().is_empty() => {
                Err("removed node alert requires alertId".into())
            }
            Self::Removed { .. } => Ok(()),
        }
    }
}
