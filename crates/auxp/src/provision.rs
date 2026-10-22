//! The provision file: what `aux provision` (agentux-core) reads to set a
//! machine up with no one at it, and what Command hands out with each
//! enrollment token. Sharing the type keeps the file Command writes one
//! that `aux provision` accepts.
//!
//! ```yaml
//! version: 1
//! profile: node
//! hostname: build-01
//! user: dev
//! command:
//!   url: https://command.acme.org
//!   enrollmentToken: auxe1_cmd-..._...
//!   nodeName: build-01
//! policy:
//!   commands:
//!     run.start: allow
//! preferences:
//!   order: [opencode, claude-code]
//! ```
//!
//! The policy and the preferences are checked by their owners: the node
//! gateway's policy parser and agentuxd's preferences.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::enrollment;

/// The provision file version this crate reads and writes.
pub const VERSION: u32 = 1;

/// Every AgentUX profile.
pub const PROFILES: &[&str] = &[
    "workstation",
    "node",
    "command",
    "executive",
    "complete",
    "custom",
];

/// The profiles that run agents, and so a node that can join Command.
pub const NODE_PROFILES: &[&str] = &["workstation", "node", "complete", "custom"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Provision {
    pub version: u32,
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// The user whose agentuxd and gateway run on the machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandJoin>,
    /// The node policy (`/etc/agentux/node-policy.yaml`), as that file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<Value>,
    /// The user's agent preferences (`~/.config/agentux/agents.yaml`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferences: Option<Value>,
}

/// The Command server to join.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandJoin {
    pub url: String,
    pub enrollment_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
}

/// A host, user or node name: letters, digits, `-`, `_`, `.`, not starting
/// with `-` or `.`, at most `max` long.
pub fn name_ok(name: &str, max: usize) -> bool {
    !name.is_empty()
        && name.len() <= max
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        && !name.starts_with(['-', '.'])
}

impl Provision {
    /// Checks everything but the policy's and the preferences' contents.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != VERSION {
            return Err(format!(
                "unsupported version {} (version {VERSION} is read)",
                self.version
            ));
        }
        if !PROFILES.contains(&self.profile.as_str()) {
            return Err(format!(
                "unknown profile `{}` (one of {})",
                self.profile,
                PROFILES.join(", ")
            ));
        }
        if let Some(host) = &self.hostname
            && !name_ok(host, 63)
        {
            return Err(format!("not a host name: {host}"));
        }
        if let Some(user) = &self.user
            && !name_ok(user, 32)
        {
            return Err(format!("not a user name: {user}"));
        }
        if let Some(join) = &self.command {
            if enrollment::token_instance(&join.enrollment_token).is_none() {
                return Err(
                    "command.enrollmentToken is not an AgentUX Command enrollment token (auxe1_...)"
                        .into(),
                );
            }
            if !NODE_PROFILES.contains(&self.profile.as_str()) {
                return Err(format!(
                    "a {} machine runs no node to join Command",
                    self.profile
                ));
            }
            if self.user.is_none() {
                return Err(
                    "joining Command needs `user`: the account whose agents run here".into(),
                );
            }
            if let Some(name) = &join.node_name
                && !name_ok(name, 63)
            {
                return Err(format!("not a node name: {name}"));
            }
        }
        if self.preferences.is_some() && self.user.is_none() {
            return Err("`preferences` need `user`".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn token() -> String {
        enrollment::token("cmd-0123456789abcdef", &"a".repeat(64))
    }

    fn node() -> Provision {
        Provision {
            version: 1,
            profile: "node".into(),
            hostname: Some("build-01".into()),
            user: Some("dev".into()),
            command: Some(CommandJoin {
                url: "https://command.acme.org".into(),
                enrollment_token: token(),
                node_name: Some("build-01".into()),
            }),
            policy: Some(json!({"commands": {"run.start": "allow"}})),
            preferences: None,
        }
    }

    #[test]
    fn what_command_writes_reads_back() {
        let file = node();
        file.validate().unwrap();
        let text = serde_json::to_string(&file).unwrap();
        assert!(text.contains("enrollmentToken") && !text.contains("preferences"));
        assert_eq!(serde_json::from_str::<Provision>(&text).unwrap(), file);
    }

    #[test]
    fn refuses_what_cannot_be_applied() {
        let mut file = node();
        file.profile = "executive".into();
        assert!(file.validate().is_err(), "an Executive runs no node");
        let mut file = node();
        file.user = None;
        assert!(file.validate().is_err(), "joining needs a user");
        let mut file = node();
        file.command.as_mut().unwrap().enrollment_token = "auxn1_x".into();
        assert!(file.validate().is_err());
        let mut file = node();
        file.version = 2;
        assert!(file.validate().is_err());
        assert!(
            serde_json::from_value::<Provision>(
                json!({"version": 1, "profile": "node", "extra": true})
            )
            .is_err()
        );
    }
}
