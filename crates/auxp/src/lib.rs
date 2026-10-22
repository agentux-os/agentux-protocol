//! AUXP v1, the AgentUX Protocol between AgentUX Command and AgentUX nodes,
//! for Rust: the node gateway (agentux-core) and the Command server
//! (agentux-command) both build on it, so they cannot drift apart.
//!
//! The JSON Schemas and fixtures in this repository are the contract; this
//! crate is tested against them.
//!
//! - [`methods`]: every method, who sends it, and the params it requires.
//! - [`validate`]: checks a message against the contract.
//! - [`Params`]: the params Command reads from node messages.
//! - [`codes`]: JSON-RPC error codes, including a node's policy answers.
//! - [`policy`]: what a node does with each command by default.
//! - [`enrollment`]: enrollment tokens, node credentials, instance ids.
//! - [`provision`]: the provision file `aux provision` reads and Command
//!   hands out with tokens.

pub mod codes;
pub mod enrollment;
pub mod methods;
pub mod policy;
pub mod provision;

mod message;

pub use message::{Params, validate};

/// The protocol version every message carries in `protocolVersion`.
pub const PROTOCOL_VERSION: &str = "v1";

/// The HTTP header with which Command names its instance on the command
/// connection; nodes compare it with the instance they enrolled with.
pub const INSTANCE_HEADER: &str = "x-agentux-command-instance";
