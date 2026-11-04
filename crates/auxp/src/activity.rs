//! Versioned, privacy-safe execution activity carried by the optional
//! `execution.activity.v1` capability.
use serde::{Deserialize, Serialize};

fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

pub const ACTIVITY_SCHEMA_VERSION: &str = "v1";
pub const MAX_SUMMARY_CHARS: usize = 2_000;
pub const MAX_ARTIFACTS: usize = 16;
pub const MAX_ARTIFACT_BYTES: u64 = 25 * 1024 * 1024;

/// Typed activity. Text is an operator-safe summary, never model reasoning,
/// raw prompts, credentials, or unrestricted tool input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub schema_version: String,
    pub activity_id: String,
    pub mission_id: String,
    pub run_id: String,
    pub agent_id: String,
    pub role_id: String,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub session_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub task_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub parent_task_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub attempt: Option<u32>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub causation_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub delegation_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_call_id: Option<String>,
    #[serde(flatten)]
    pub kind: ActivityKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ActivityKind {
    PlanCreated {
        summary: String,
    },
    TaskCreated {
        task_name: String,
        status: TaskStatus,
        summary: String,
    },
    TaskStatusChanged {
        status: TaskStatus,
        summary: String,
    },
    ToolStarted {
        tool_name: String,
        summary: String,
    },
    ToolProgress {
        tool_name: String,
        progress: u8,
        summary: String,
    },
    ToolCompleted {
        tool_name: String,
        summary: String,
    },
    ToolFailed {
        tool_name: String,
        summary: String,
    },
    TaskAssigned {
        to_agent_id: String,
        to_role_id: String,
        summary: String,
    },
    Delegated {
        to_agent_id: String,
        to_role_id: String,
        summary: String,
    },
    Handoff {
        to_agent_id: String,
        to_role_id: String,
        summary: String,
    },
    ResultReturned {
        from_agent_id: String,
        summary: String,
    },
    TaskCompleted {
        summary: String,
    },
    TaskFailed {
        summary: String,
    },
    ArtifactAvailable {
        summary: String,
        artifacts: Vec<ArtifactRef>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

/// A bounded reference to content stored out of band. The payload is never
/// embedded in an activity event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactRef {
    pub artifact_id: String,
    pub media_type: String,
    pub byte_length: u64,
    pub sha256: String,
    pub uri: String,
}

impl Activity {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != ACTIVITY_SCHEMA_VERSION {
            return Err("unsupported activity schemaVersion".into());
        }
        for (key, value) in [
            ("activityId", &self.activity_id),
            ("missionId", &self.mission_id),
            ("runId", &self.run_id),
            ("agentId", &self.agent_id),
            ("roleId", &self.role_id),
        ] {
            if value.is_empty() || value.chars().count() > 128 {
                return Err(format!("{key} must contain 1..=128 characters"));
            }
        }
        for (key, value) in [
            ("sessionId", self.session_id.as_deref()),
            ("taskId", self.task_id.as_deref()),
            ("parentTaskId", self.parent_task_id.as_deref()),
            ("causationId", self.causation_id.as_deref()),
            ("delegationId", self.delegation_id.as_deref()),
            ("toolCallId", self.tool_call_id.as_deref()),
        ] {
            if value.is_some_and(|v| v.is_empty() || v.chars().count() > 128) {
                return Err(format!(
                    "{key} must contain 1..=128 characters when present"
                ));
            }
        }
        if self.attempt == Some(0) {
            return Err("attempt must be positive".into());
        }
        if matches!(
            &self.kind,
            ActivityKind::TaskCreated { .. }
                | ActivityKind::TaskStatusChanged { .. }
                | ActivityKind::TaskCompleted { .. }
                | ActivityKind::TaskFailed { .. }
        ) && self.task_id.as_deref().is_none_or(str::is_empty)
        {
            return Err("task activity requires taskId".into());
        }
        if matches!(
            &self.kind,
            ActivityKind::TaskAssigned { .. }
                | ActivityKind::Delegated { .. }
                | ActivityKind::Handoff { .. }
                | ActivityKind::ResultReturned { .. }
        ) && self.delegation_id.as_deref().is_none_or(str::is_empty)
        {
            return Err("delegation activity requires delegationId".into());
        }
        if matches!(
            &self.kind,
            ActivityKind::ToolStarted { .. }
                | ActivityKind::ToolProgress { .. }
                | ActivityKind::ToolCompleted { .. }
                | ActivityKind::ToolFailed { .. }
        ) && self.tool_call_id.as_deref().is_none_or(str::is_empty)
        {
            return Err("tool activity requires toolCallId".into());
        }
        let summary = match &self.kind {
            ActivityKind::PlanCreated { summary }
            | ActivityKind::TaskCreated { summary, .. }
            | ActivityKind::TaskStatusChanged { summary, .. }
            | ActivityKind::ToolStarted { summary, .. }
            | ActivityKind::ToolProgress { summary, .. }
            | ActivityKind::ToolCompleted { summary, .. }
            | ActivityKind::ToolFailed { summary, .. }
            | ActivityKind::TaskAssigned { summary, .. }
            | ActivityKind::Delegated { summary, .. }
            | ActivityKind::Handoff { summary, .. }
            | ActivityKind::ResultReturned { summary, .. }
            | ActivityKind::TaskCompleted { summary }
            | ActivityKind::TaskFailed { summary }
            | ActivityKind::ArtifactAvailable { summary, .. } => summary,
        };
        if summary.chars().count() > MAX_SUMMARY_CHARS {
            return Err("activity summary exceeds 2000 characters".into());
        }
        if let ActivityKind::ToolProgress { progress, .. } = &self.kind
            && *progress > 100
        {
            return Err("tool progress must be 0..=100".into());
        }
        let bounded_variant_text: Vec<(&str, &str, usize)> = match &self.kind {
            ActivityKind::TaskCreated { task_name, .. } => vec![("taskName", task_name, 256)],
            ActivityKind::ToolStarted { tool_name, .. }
            | ActivityKind::ToolProgress { tool_name, .. }
            | ActivityKind::ToolCompleted { tool_name, .. }
            | ActivityKind::ToolFailed { tool_name, .. } => vec![("toolName", tool_name, 128)],
            ActivityKind::TaskAssigned {
                to_agent_id,
                to_role_id,
                ..
            }
            | ActivityKind::Delegated {
                to_agent_id,
                to_role_id,
                ..
            }
            | ActivityKind::Handoff {
                to_agent_id,
                to_role_id,
                ..
            } => vec![
                ("toAgentId", to_agent_id, 128),
                ("toRoleId", to_role_id, 128),
            ],
            ActivityKind::ResultReturned { from_agent_id, .. } => {
                vec![("fromAgentId", from_agent_id, 128)]
            }
            _ => Vec::new(),
        };
        for (key, value, max) in bounded_variant_text {
            if value.is_empty() || value.chars().count() > max {
                return Err(format!("{key} must contain 1..={max} characters"));
            }
        }
        if let ActivityKind::ArtifactAvailable { artifacts, .. } = &self.kind {
            if artifacts.len() > MAX_ARTIFACTS {
                return Err("activity has more than 16 artifact references".into());
            }
            for artifact in artifacts {
                if artifact.artifact_id.is_empty()
                    || artifact.artifact_id.chars().count() > 128
                    || artifact.media_type.is_empty()
                    || artifact.media_type.chars().count() > 127
                    || artifact.uri.chars().count() > 2048
                {
                    return Err("artifact identity, media type or URI exceeds its limit".into());
                }
                if artifact.byte_length > MAX_ARTIFACT_BYTES {
                    return Err("artifact exceeds 25 MiB".into());
                }
                if artifact.sha256.len() != 64
                    || !artifact.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err("artifact sha256 must be 64 hexadecimal characters".into());
                }
                if !(artifact.uri.starts_with("https://")
                    || artifact.uri.starts_with("artifact://"))
                {
                    return Err("artifact URI must use https or artifact scheme".into());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn activity_json_is_typed_bounded_and_versioned() {
        let value = json!({"schemaVersion":"v1","activityId":"a1","missionId":"m1","runId":"r1","agentId":"agent-a","roleId":"worker","taskId":"t1","type":"task_created","taskName":"Build","status":"pending","summary":"Safe status"});
        let activity: Activity = serde_json::from_value(value.clone()).unwrap();
        activity.validate().unwrap();
        let mut invalid = value;
        invalid["schemaVersion"] = json!("v2");
        assert!(
            serde_json::from_value::<Activity>(invalid)
                .unwrap()
                .validate()
                .is_err()
        );
        let mut invalid = json!({"schemaVersion":"v1","activityId":"a1","missionId":"m1","runId":"r1","agentId":"agent-a","roleId":"worker","type":"tool_progress","toolName":"search","toolCallId":"tool-1","progress":101,"summary":"working"});
        assert!(
            serde_json::from_value::<Activity>(invalid.take())
                .unwrap()
                .validate()
                .is_err()
        );
    }

    #[test]
    fn identifier_limits_count_unicode_characters_like_json_schema() {
        let mut value = json!({"schemaVersion":"v1","activityId":"é".repeat(128),"missionId":"m","runId":"r","agentId":"a","roleId":"worker","type":"plan_created","summary":"ok"});
        serde_json::from_value::<Activity>(value.clone())
            .unwrap()
            .validate()
            .unwrap();
        value["activityId"] = json!("é".repeat(129));
        assert!(
            serde_json::from_value::<Activity>(value)
                .unwrap()
                .validate()
                .is_err()
        );
    }

    #[test]
    fn optional_activity_fields_reject_null_like_the_schema() {
        let value = json!({"schemaVersion":"v1","activityId":"a","missionId":"m","runId":"r","agentId":"a","roleId":"worker","sessionId":null,"type":"plan_created","summary":"ok"});
        assert!(serde_json::from_value::<Activity>(value).is_err());
    }

    #[test]
    fn artifact_reference_bounds_and_uri_are_enforced() {
        let activity = Activity {
            schema_version: "v1".into(),
            activity_id: "a".into(),
            mission_id: "m".into(),
            run_id: "r".into(),
            agent_id: "agent".into(),
            role_id: "worker".into(),
            session_id: None,
            task_id: None,
            parent_task_id: None,
            attempt: None,
            causation_id: None,
            delegation_id: None,
            tool_call_id: None,
            kind: ActivityKind::ArtifactAvailable {
                summary: "report ready".into(),
                artifacts: vec![ArtifactRef {
                    artifact_id: "report".into(),
                    media_type: "text/plain".into(),
                    byte_length: 10,
                    sha256: "a".repeat(64),
                    uri: "file:///private/report".into(),
                }],
            },
        };
        assert!(activity.validate().is_err());
    }
}
