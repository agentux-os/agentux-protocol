//! The JSON Schemas of this repository, as text, for consumers' contract
//! tests (Command checks its Office projection and approval answers against
//! them). The files under `schemas/v1` are the source; these are those files.

/// The AUXP envelope every message shares.
pub const ENVELOPE: &str = include_str!("../../../schemas/v1/envelope.schema.json");
/// Every AUXP method and its params.
pub const MESSAGES: &str = include_str!("../../../schemas/v1/messages.schema.json");
/// Enrollment tokens, redemption and its answers.
pub const ENROLLMENT: &str = include_str!("../../../schemas/v1/enrollment.schema.json");
/// The Office projection (`office-projection.v1`).
pub const OFFICE_PROJECTION: &str =
    include_str!("../../../schemas/v1/office-projection.schema.json");
/// Approval resolution requests and answers (`approval-resolution.v1`).
pub const APPROVAL_RESOLUTION: &str =
    include_str!("../../../schemas/v1/approval-resolution.schema.json");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_schema_is_json_with_an_id() {
        for schema in [
            ENVELOPE,
            MESSAGES,
            ENROLLMENT,
            OFFICE_PROJECTION,
            APPROVAL_RESOLUTION,
        ] {
            let value: serde_json::Value = serde_json::from_str(schema).unwrap();
            assert!(
                value["$id"]
                    .as_str()
                    .is_some_and(|id| id.ends_with(".schema.json"))
            );
        }
    }
}
