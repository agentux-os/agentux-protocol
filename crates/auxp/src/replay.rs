//! Command → node event replay and durable acknowledgement contracts.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayStatus {
    Replay,
    CursorExpired,
}

/// JSON-RPC result for Command's `node.resume` request. Replayed event
/// notifications travel from the node to Command on the event channel; they
/// are deliberately not embedded here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplayResult {
    pub status: ReplayStatus,
    pub node_id: String,
    pub requested_after_sequence: i64,
    pub oldest_available_sequence: i64,
    pub high_water_sequence: i64,
    pub events_replayed: i64,
    pub snapshot_required: bool,
}

impl ReplayResult {
    pub fn validate(&self) -> Result<(), String> {
        if self.node_id.is_empty() || self.node_id.chars().count() > 128 {
            return Err("nodeId must contain 1..=128 characters".into());
        }
        if self.requested_after_sequence < 0
            || self.oldest_available_sequence < 0
            || self.high_water_sequence < 0
            || self.events_replayed < 0
        {
            return Err("replay sequence values must be non-negative".into());
        }
        if self.high_water_sequence < self.requested_after_sequence {
            return Err("highWaterSequence cannot precede requestedAfterSequence".into());
        }
        if self.oldest_available_sequence > self.high_water_sequence.saturating_add(1) {
            return Err("oldestAvailableSequence cannot exceed highWaterSequence + 1".into());
        }
        match self.status {
            ReplayStatus::Replay => {
                if self.snapshot_required {
                    return Err("replay status cannot require a snapshot".into());
                }
                if self.requested_after_sequence.saturating_add(1) < self.oldest_available_sequence
                {
                    return Err("an expired cursor must return cursor_expired".into());
                }
                if self.events_replayed != self.high_water_sequence - self.requested_after_sequence
                {
                    return Err(
                        "replay must enqueue every sequence through highWaterSequence".into(),
                    );
                }
            }
            ReplayStatus::CursorExpired => {
                if !self.snapshot_required || self.events_replayed != 0 {
                    return Err(
                        "cursor_expired requires snapshotRequired and zero replayed events".into(),
                    );
                }
                if self.requested_after_sequence.saturating_add(1) >= self.oldest_available_sequence
                {
                    return Err("cursor_expired requires a cursor before the retained range".into());
                }
            }
        }
        Ok(())
    }
}

/// Command acknowledges only the highest contiguous sequence it has
/// durably committed. This cursor is scoped to the authenticated node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventAck {
    pub through_sequence: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn result(
        status: ReplayStatus,
        after: i64,
        oldest: i64,
        high: i64,
        count: i64,
        snapshot: bool,
    ) -> ReplayResult {
        ReplayResult {
            status,
            node_id: "node-a".into(),
            requested_after_sequence: after,
            oldest_available_sequence: oldest,
            high_water_sequence: high,
            events_replayed: count,
            snapshot_required: snapshot,
        }
    }

    #[test]
    fn replay_result_accepts_a_replay_and_rejects_invalid_cursor_ranges() {
        assert!(
            result(ReplayStatus::Replay, 4, 2, 8, 4, false)
                .validate()
                .is_ok()
        );
        assert!(
            result(ReplayStatus::Replay, 1, 4, 8, 7, false)
                .validate()
                .is_err()
        );
        assert!(
            result(ReplayStatus::Replay, 4, 2, 8, 5, false)
                .validate()
                .is_err()
        );
        assert!(
            result(ReplayStatus::Replay, -1, 0, 8, 9, false)
                .validate()
                .is_err()
        );
    }

    #[test]
    fn expired_cursor_requires_snapshot_and_must_not_claim_replayed_events() {
        assert!(
            result(ReplayStatus::CursorExpired, 1, 4, 8, 0, true)
                .validate()
                .is_ok()
        );
        assert!(
            result(ReplayStatus::CursorExpired, 1, 4, 8, 1, true)
                .validate()
                .is_err()
        );
        assert!(
            result(ReplayStatus::CursorExpired, 4, 4, 8, 0, true)
                .validate()
                .is_err()
        );
        let parsed: ReplayResult = serde_json::from_value(json!({"status":"cursor_expired","nodeId":"node-a","requestedAfterSequence":1,"oldestAvailableSequence":4,"highWaterSequence":8,"eventsReplayed":0,"snapshotRequired":true})).unwrap();
        assert!(parsed.validate().is_ok());
    }
}
