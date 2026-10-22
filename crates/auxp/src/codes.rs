//! JSON-RPC error codes in AUXP answers.

/// The node does not implement the method.
pub const METHOD_NOT_FOUND: i64 = -32601;
/// The params are invalid.
pub const INVALID_PARAMS: i64 = -32602;
/// The node's policy holds the command until someone at the node confirms
/// it; a `command.completed` event follows.
pub const HELD: i64 = -32010;
/// The node's policy refuses the command.
pub const DENIED: i64 = -32011;
/// The node's daemon refused or failed the command.
pub const FAILED: i64 = -32012;

/// Whether an error is the node's policy speaking (held or denied), as
/// opposed to a failure.
pub fn is_policy(code: i64) -> bool {
    code == HELD || code == DENIED
}
