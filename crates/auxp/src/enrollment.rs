//! How a node joins one Command server: the server's instance id, the
//! single-use enrollment tokens it issues, and the node credentials it
//! trades them for.
//!
//! An enrollment token names its server (`auxe1_<instanceId>_<secret>`), so
//! a node can check it reached that server before sending the token. A node
//! credential (`auxn1_<secret>`) is bound to one node, organization and
//! server.

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
