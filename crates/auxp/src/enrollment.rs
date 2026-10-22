//! How a node joins one Command server: the server's instance id, the
//! single-use enrollment tokens it issues, and the node credentials it
//! trades them for.
//!
//! An enrollment token names its server (`auxe1_<instanceId>_<secret>`), so
//! a node can check it reached that server before sending the token. A node
//! credential (`auxn1_<secret>`) is bound to one node, organization and
//! server.

use serde::{Deserialize, Serialize};

/// Enrollment tokens start with this.
pub const TOKEN_PREFIX: &str = "auxe1_";
/// Node credentials start with this.
pub const CREDENTIAL_PREFIX: &str = "auxn1_";
/// The hex digits of a token's or a credential's secret.
pub const SECRET_HEX_LEN: usize = 64;

/// `cmd-` and 16 lowercase hex digits.
pub fn valid_instance_id(id: &str) -> bool {
    id.strip_prefix("cmd-")
        .is_some_and(|hex| hex.len() == 16 && is_lower_hex(hex))
}

/// The instance id an enrollment token names; `None` when it is not one.
pub fn token_instance(token: &str) -> Option<&str> {
    let rest = token.trim().strip_prefix(TOKEN_PREFIX)?;
    let (instance, secret) = rest.split_once('_')?;
    (valid_instance_id(instance) && secret.len() == SECRET_HEX_LEN && is_lower_hex(secret))
        .then_some(instance)
}

/// Whether `token` looks like a node credential (it is checked by Command).
pub fn is_credential(token: &str) -> bool {
    token.starts_with(CREDENTIAL_PREFIX)
}

/// An enrollment token for `instance` with `secret_hex` (from a random
/// source).
pub fn token(instance: &str, secret_hex: &str) -> String {
    format!("{TOKEN_PREFIX}{instance}_{secret_hex}")
}

/// A node credential with `secret_hex`.
pub fn credential(secret_hex: &str) -> String {
    format!("{CREDENTIAL_PREFIX}{secret_hex}")
}

/// Whether `credential` has the shape of a node credential.
pub fn valid_credential(credential: &str) -> bool {
    credential
        .strip_prefix(CREDENTIAL_PREFIX)
        .is_some_and(|secret| secret.len() == SECRET_HEX_LEN && is_lower_hex(secret))
}

/// Whether `id` is a node id: lowercase letters, digits, `.`, `_`, `-`,
/// starting with a letter or digit, at most 63 long.
pub fn valid_node_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(b))
}

/// `GET /v1/instance`: who a Command server is. Nodes compare it with the
/// instance their token names before sending the token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub product: String,
    pub instance_id: String,
    pub protocol_versions: Vec<String>,
}

impl Instance {
    pub fn new(instance_id: &str) -> Self {
        Self {
            product: PRODUCT.into(),
            instance_id: instance_id.into(),
            protocol_versions: vec![crate::PROTOCOL_VERSION.into()],
        }
    }

    pub fn check(&self) -> Result<(), String> {
        if self.product != PRODUCT {
            return Err(format!("not an AgentUX Command server: {}", self.product));
        }
        if !valid_instance_id(&self.instance_id) {
            return Err(format!("not an instance id: {}", self.instance_id));
        }
        if !self
            .protocol_versions
            .iter()
            .any(|v| v == crate::PROTOCOL_VERSION)
        {
            return Err("the server does not speak AUXP v1".into());
        }
        Ok(())
    }
}

/// The product name `GET /v1/instance` answers with.
pub const PRODUCT: &str = "agentux-command";

/// `POST /v1/enroll`: a node redeems a token. `nodeName` and `hostname` may
/// be null.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Redemption {
    pub token: String,
    #[serde(default)]
    pub node_name: Option<String>,
    #[serde(default)]
    pub hostname: Option<String>,
}

impl Redemption {
    pub fn check(&self) -> Result<(), String> {
        token_instance(&self.token)
            .map(|_| ())
            .ok_or_else(|| "not an enrollment token".into())
    }
}

/// The answer to a redemption: the node's identity and its credential, the
/// only time the credential is ever returned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Enrolled {
    pub instance_id: String,
    pub organization_id: String,
    pub node_id: String,
    pub node_secret: String,
    /// False while an operator still has to approve the node.
    pub approved: bool,
}

impl Enrolled {
    pub fn check(&self) -> Result<(), String> {
        if !valid_instance_id(&self.instance_id) {
            return Err(format!("not an instance id: {}", self.instance_id));
        }
        if self.organization_id.is_empty() {
            return Err("organizationId is required".into());
        }
        if !valid_node_id(&self.node_id) {
            return Err(format!("not a node id: {}", self.node_id));
        }
        if !valid_credential(&self.node_secret) {
            return Err("nodeSecret is not a node credential".into());
        }
        Ok(())
    }
}

/// A refused redemption (401 or 409): a reason, and this server's instance
/// when the token belongs to another one. Never a credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Refused {
    pub error: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
}

fn is_lower_hex(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSTANCE: &str = "cmd-0123456789abcdef";

    #[test]
    fn tokens_name_their_instance() {
        let t = token(INSTANCE, &"a".repeat(64));
        assert_eq!(token_instance(&t), Some(INSTANCE));
        assert_eq!(token_instance(&format!(" {t}\n")), Some(INSTANCE));
        assert_eq!(
            token_instance(&format!("{TOKEN_PREFIX}{INSTANCE}_short")),
            None
        );
        assert_eq!(
            token_instance(&format!("{TOKEN_PREFIX}cmd-XYZ_{}", "a".repeat(64))),
            None
        );
        assert_eq!(
            token_instance(&format!("{TOKEN_PREFIX}{INSTANCE}_{}", "G".repeat(64))),
            None
        );
        assert_eq!(token_instance(&credential(&"a".repeat(64))), None);
    }

    #[test]
    fn instance_ids_have_one_shape() {
        assert!(valid_instance_id(INSTANCE));
        assert!(!valid_instance_id("cmd-0123456789ABCDEF"));
        assert!(!valid_instance_id("cmd-0123"));
        assert!(!valid_instance_id("0123456789abcdef"));
    }
}
