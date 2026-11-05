//! Atomic, node-wide snapshot checkpoint pages and whole-chain validation.
//!
//! Receivers must collect the raw bytes of every page and call
//! [`validate_page_chain`] before changing any projection or sending a
//! checkpoint-bound event ACK. The returned chain represents the only safe
//! input to that durable replacement transaction.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_PAGE_ENTRIES: usize = 100;
pub const MAX_PAGE_BYTES: usize = 1_048_576;
pub const MAX_SNAPSHOT_ENTRIES: usize = 10_000;
pub const MAX_SNAPSHOT_BYTES: usize = 67_108_864;
pub const MAX_PAGES: usize = 10_000;

/// Every node-owned current-state projection changed by a sequenced node event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionType {
    Approval,
    Artifact,
    Command,
    Gate,
    Harness,
    Limit,
    Node,
    NodeAlert,
    Run,
    Session,
    Task,
}

pub const PROJECTION_COVERAGE: &[ProjectionType] = &[
    ProjectionType::Approval,
    ProjectionType::Artifact,
    ProjectionType::Command,
    ProjectionType::Gate,
    ProjectionType::Harness,
    ProjectionType::Limit,
    ProjectionType::Node,
    ProjectionType::NodeAlert,
    ProjectionType::Run,
    ProjectionType::Session,
    ProjectionType::Task,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    State,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotEntry {
    pub kind: EntryKind,
    pub projection_type: ProjectionType,
    pub entry_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed_at_sequence: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotPage {
    pub schema_version: String,
    pub checkpoint_id: String,
    pub snapshot_id: String,
    pub organization_id: String,
    pub node_id: String,
    pub requested_after_sequence: i64,
    pub high_water_sequence: i64,
    pub projection_coverage: Vec<ProjectionType>,
    pub page_index: usize,
    pub page_count: usize,
    pub entry_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    pub entries: Vec<SnapshotEntry>,
}

/// A fully parsed and validated page chain. Its constructor is private so a
/// receiver cannot accidentally treat a partial page list as a checkpoint.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotPageChain {
    pages: Vec<SnapshotPage>,
}

impl SnapshotPageChain {
    pub fn pages(&self) -> &[SnapshotPage] {
        &self.pages
    }

    pub fn checkpoint_id(&self) -> &str {
        &self.pages[0].checkpoint_id
    }

    pub fn snapshot_id(&self) -> &str {
        &self.pages[0].snapshot_id
    }

    pub fn organization_id(&self) -> &str {
        &self.pages[0].organization_id
    }

    pub fn node_id(&self) -> &str {
        &self.pages[0].node_id
    }

    pub fn requested_after_sequence(&self) -> i64 {
        self.pages[0].requested_after_sequence
    }

    pub fn high_water_sequence(&self) -> i64 {
        self.pages[0].high_water_sequence
    }

    pub fn entries(&self) -> impl Iterator<Item = &SnapshotEntry> {
        self.pages.iter().flat_map(|page| page.entries.iter())
    }

    /// Confirms the wire binding for an ACK after the receiver has committed
    /// this entire validated chain in one durable transaction.
    pub fn validate_checkpoint_ack(
        &self,
        organization_id: &str,
        node_id: &str,
        checkpoint_id: &str,
        through_sequence: i64,
        last_accepted_sequence: i64,
    ) -> Result<(), String> {
        if organization_id != self.organization_id() || node_id != self.node_id() {
            return Err("checkpoint ACK scope does not match snapshot".into());
        }
        if checkpoint_id != self.checkpoint_id() {
            return Err("checkpoint ACK id does not match snapshot".into());
        }
        if self.requested_after_sequence() != last_accepted_sequence {
            return Err("snapshot starting cursor is stale relative to last accepted ACK".into());
        }
        if through_sequence != self.high_water_sequence() {
            return Err("checkpoint ACK must equal highWaterSequence".into());
        }
        Ok(())
    }
}

/// Parse and validate the complete set of raw JSON-RPC result payloads.
///
/// `encoded_pages` must contain the exact bytes received from the node, in
/// request order. This validates byte bounds as well as the immutable
/// manifest, page indexes/count, token chain, entry ordering/uniqueness,
/// complete projection coverage, tombstones and sequence boundaries. A
/// caller must not apply individual pages before this returns `Ok`.
pub fn validate_page_chain(encoded_pages: &[&[u8]]) -> Result<SnapshotPageChain, String> {
    if encoded_pages.is_empty() || encoded_pages.len() > MAX_PAGES {
        return Err("snapshot page chain must contain 1..=10000 pages".into());
    }
    let mut total_bytes = 0usize;
    for bytes in encoded_pages {
        if bytes.len() > MAX_PAGE_BYTES {
            return Err("snapshot page exceeds 1 MiB encoded limit".into());
        }
        total_bytes = total_bytes.saturating_add(bytes.len());
        if total_bytes > MAX_SNAPSHOT_BYTES {
            return Err("snapshot chain exceeds 64 MiB encoded limit".into());
        }
    }
    let pages: Vec<SnapshotPage> = encoded_pages
        .iter()
        .map(|bytes| {
            serde_json::from_slice::<SnapshotPage>(bytes)
                .map_err(|error| format!("invalid snapshot page: {error}"))
        })
        .collect::<Result<_, _>>()?;

    let first = &pages[0];
    if first.schema_version != "v1"
        || first.checkpoint_id.is_empty()
        || first.snapshot_id.is_empty()
        || first.organization_id.is_empty()
        || first.node_id.is_empty()
    {
        return Err("snapshot identity and schemaVersion are required".into());
    }
    if first.requested_after_sequence < 0
        || first.high_water_sequence < first.requested_after_sequence
    {
        return Err(
            "requestedAfterSequence must be non-negative and no greater than highWaterSequence"
                .into(),
        );
    }
    if first.page_count == 0
        || first.page_count > MAX_PAGES
        || first.page_count != pages.len()
        || first.entry_count > MAX_SNAPSHOT_ENTRIES
    {
        return Err("snapshot pageCount or entryCount is invalid".into());
    }
    if first.projection_coverage.as_slice() != PROJECTION_COVERAGE {
        return Err("snapshot projectionCoverage is incomplete or out of canonical order".into());
    }

    let mut prior_entry_id: Option<&str> = None;
    let mut actual_entries = 0usize;
    for (index, page) in pages.iter().enumerate() {
        if page.page_index >= page.page_count || page.page_index != index {
            return Err(
                "pageIndex must equal its chain position and be less than pageCount".into(),
            );
        }
        if page.page_count != pages.len() || page.page_count != first.page_count {
            return Err("pageCount does not match the complete page chain".into());
        }
        if page.schema_version != first.schema_version
            || page.checkpoint_id != first.checkpoint_id
            || page.snapshot_id != first.snapshot_id
            || page.organization_id != first.organization_id
            || page.node_id != first.node_id
            || page.requested_after_sequence != first.requested_after_sequence
            || page.high_water_sequence != first.high_water_sequence
            || page.entry_count != first.entry_count
            || page.projection_coverage != first.projection_coverage
        {
            return Err("snapshot page immutable manifest changed within chain".into());
        }
        let expected_page_token = index
            .checked_sub(1)
            .map(|previous| pages[previous].next_page_token.as_deref().unwrap_or(""));
        if page.page_token.as_deref() != expected_page_token {
            return Err("pageToken does not continue the previous nextPageToken".into());
        }
        if index + 1 < pages.len() {
            if page.next_page_token.as_deref().is_none_or(str::is_empty) {
                return Err("non-final page must carry nextPageToken".into());
            }
        } else if page.next_page_token.is_some() {
            return Err("final page must omit nextPageToken".into());
        }
        if page.entries.len() > MAX_PAGE_ENTRIES {
            return Err("snapshot page exceeds 100 entries".into());
        }
        actual_entries = actual_entries.saturating_add(page.entries.len());
        for entry in &page.entries {
            validate_entry(
                entry,
                first.requested_after_sequence,
                first.high_water_sequence,
                &first.organization_id,
                &first.node_id,
            )?;
            if prior_entry_id.is_some_and(|prior| prior >= entry.entry_id.as_str()) {
                return Err(
                    "snapshot entry IDs must be globally unique and strictly ordered".into(),
                );
            }
            prior_entry_id = Some(&entry.entry_id);
        }
    }
    if actual_entries != first.entry_count {
        return Err("entryCount does not match the concatenated page entries".into());
    }
    Ok(SnapshotPageChain { pages })
}

fn projection_name(kind: ProjectionType) -> &'static str {
    match kind {
        ProjectionType::Approval => "approval",
        ProjectionType::Artifact => "artifact",
        ProjectionType::Command => "command",
        ProjectionType::Gate => "gate",
        ProjectionType::Harness => "harness",
        ProjectionType::Limit => "limit",
        ProjectionType::Node => "node",
        ProjectionType::NodeAlert => "node_alert",
        ProjectionType::Run => "run",
        ProjectionType::Session => "session",
        ProjectionType::Task => "task",
    }
}

fn validate_entry(
    entry: &SnapshotEntry,
    requested: i64,
    high_water: i64,
    organization_id: &str,
    node_id: &str,
) -> Result<(), String> {
    let prefix = format!("{}:", projection_name(entry.projection_type));
    let Some(identity) = entry.entry_id.strip_prefix(&prefix) else {
        return Err("entryId must use the projectionType:id mapping".into());
    };
    if identity.is_empty() {
        return Err("entryId mapping must contain a stable projection ID".into());
    }
    match entry.kind {
        EntryKind::State => {
            if entry.removed_at_sequence.is_some() || entry.run_id.is_some() {
                return Err("state entries cannot carry tombstone fields".into());
            }
            let state = entry
                .state
                .as_ref()
                .and_then(Value::as_object)
                .ok_or("state entry requires a complete object")?;
            if state_identity(entry.projection_type, state).is_none_or(|id| id != identity) {
                return Err("state object's stable ID does not match entryId".into());
            }
            if entry.projection_type == ProjectionType::Node && identity != node_id {
                return Err("node projection entryId must identify the authenticated node".into());
            }
            if !state
                .get("currentState")
                .and_then(Value::as_object)
                .is_some_and(|current| !current.is_empty())
            {
                return Err("state entry must contain its complete currentState object".into());
            }
            let current = state
                .get("currentState")
                .and_then(Value::as_object)
                .ok_or("state entry must contain its complete currentState object")?;
            match entry.projection_type {
                ProjectionType::Node => {
                    for node in [state, current] {
                        if !matches!(
                            node.get("status").and_then(Value::as_str),
                            Some("ready" | "draining" | "error")
                        ) || !node.get("capacity").is_some_and(Value::is_object)
                        {
                            return Err(
                                "node snapshot state requires supported status and object capacity"
                                    .into(),
                            );
                        }
                    }
                    if current.get("nodeId") != state.get("nodeId") {
                        return Err(
                            "node snapshot currentState nodeId must match the projection".into(),
                        );
                    }
                }
                ProjectionType::NodeAlert => {
                    if current.get("alertId") != state.get("alertId") {
                        return Err(
                            "node alert snapshot currentState alertId must match the projection"
                                .into(),
                        );
                    }
                    for alert in [state, current] {
                        for field in ["alertId", "producer", "code", "conditionKey", "summary"] {
                            if !alert.get(field).is_some_and(non_empty_string) {
                                return Err(format!(
                                    "node alert snapshot state requires non-empty {field}"
                                ));
                            }
                        }
                        if !matches!(
                            alert.get("status").and_then(Value::as_str),
                            Some("active" | "resolved")
                        ) {
                            return Err(
                                "node alert snapshot state requires active or resolved status"
                                    .into(),
                            );
                        }
                        let summary = alert.get("summary").and_then(Value::as_str).unwrap();
                        if summary.chars().count() > 2_000 {
                            return Err(
                                "node alert snapshot summary exceeds 2000 Unicode characters"
                                    .into(),
                            );
                        }
                        let first: jiff::Timestamp = alert
                            .get("firstObservedAt")
                            .and_then(Value::as_str)
                            .ok_or("node alert snapshot requires firstObservedAt")?
                            .parse()
                            .map_err(|error| {
                                format!("node alert firstObservedAt must be RFC3339: {error}")
                            })?;
                        let last: jiff::Timestamp = alert
                            .get("lastObservedAt")
                            .and_then(Value::as_str)
                            .ok_or("node alert snapshot requires lastObservedAt")?
                            .parse()
                            .map_err(|error| {
                                format!("node alert lastObservedAt must be RFC3339: {error}")
                            })?;
                        if last < first {
                            return Err(
                                "node alert lastObservedAt must not precede firstObservedAt".into(),
                            );
                        }
                    }
                }
                _ => {}
            }
            if matches!(
                entry.projection_type,
                ProjectionType::Run
                    | ProjectionType::Session
                    | ProjectionType::Gate
                    | ProjectionType::Task
                    | ProjectionType::Approval
                    | ProjectionType::Artifact
                    | ProjectionType::Command
            ) && state.get("organizationId").and_then(Value::as_str) != Some(organization_id)
            {
                return Err(
                    "projection entry is outside the authenticated organization scope".into(),
                );
            }
            if entry.projection_type == ProjectionType::Run
                && (!state
                    .get("owner")
                    .and_then(Value::as_str)
                    .is_some_and(|owner| owner == "command")
                    || !state.get("commandId").is_some_and(non_empty_string)
                    || !state.get("missionId").is_some_and(non_empty_string)
                    || !state.get("status").is_some_and(non_empty_string)
                    || !state.get("currentState").is_some_and(Value::is_object))
            {
                return Err(
                    "run state must include missionId, status and complete currentState".into(),
                );
            }
        }
        EntryKind::Removed => {
            if entry.state.is_some() {
                return Err("removed entries cannot carry current state".into());
            }
            entry
                .removed_at_sequence
                .filter(|sequence| *sequence > requested && *sequence <= high_water)
                .ok_or(
                    "tombstone sequence must be within (requestedAfterSequence, highWaterSequence]",
                )?;
            if entry.projection_type == ProjectionType::Run
                && entry.run_id.as_deref() != Some(identity)
            {
                return Err(
                    "run tombstone requires runId matching entryId's run:<runId> mapping".into(),
                );
            }
            if entry.projection_type != ProjectionType::Run && entry.run_id.is_some() {
                return Err("runId is only valid on run entries".into());
            }
        }
    }
    Ok(())
}

fn state_identity(kind: ProjectionType, state: &serde_json::Map<String, Value>) -> Option<&str> {
    let field = match kind {
        ProjectionType::Approval => "approvalId",
        ProjectionType::Artifact => "artifactId",
        ProjectionType::Command => "commandId",
        ProjectionType::Gate => "gateId",
        ProjectionType::Harness => "harnessId",
        ProjectionType::Limit => "limitId",
        ProjectionType::Node => "nodeId",
        ProjectionType::NodeAlert => "alertId",
        ProjectionType::Run => "runId",
        ProjectionType::Session => "sessionId",
        ProjectionType::Task => "taskId",
    };
    state.get(field).and_then(Value::as_str)
}

fn non_empty_string(value: &Value) -> bool {
    value.as_str().is_some_and(|text| !text.is_empty())
}
