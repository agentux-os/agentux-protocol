# Protocol #18: typed node status and alert events Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Define and validate complete, sequence-owned AUXP v1 payloads for node status and local alert changes, aligned with node snapshot state.

**Architecture:** Keep the existing JSON-RPC event names and common `eventId`/`sequence` envelope. Add typed `node` and `alert` payloads, validate them in Rust and fixtures, and share their state vocabulary with snapshot schemas. Heartbeat repeats node state without becoming a sequenced mutation; Command derives fleet presence from heartbeat freshness.

**Tech Stack:** Rust 2024, Serde, JSON Schema 2020-12, JSON fixtures, Node.js fixture validator.

**Spec:** `docs/superpowers/specs/2026-11-05-protocol-18-node-status-alert-events-design.md`

## Global Constraints

- `node.status_changed` is a full-state replacement for `status` and `capacity`; changes to either use a positive event sequence.
- Operational node statuses are `ready`, `draining`, and `error`; fleet `offline` is derived from heartbeat freshness.
- `node.heartbeat` repeats committed status/capacity and does not carry `eventId` or `sequence`.
- Alert lifecycle statuses are `active` and `resolved`; removal is an explicit sequenced tombstone keyed by stable `alertId`.
- Alert summaries are operator-safe, non-empty after trimming, and bounded to 2,000 Unicode characters; payloads have closed fields and contain no secrets or unrestricted logs.
- Snapshot capability remains gated until Core has durable sources for every covered projection.

## Review Focus

- A status payload whose `nodeId` differs from the authenticated envelope is rejected; pin this in Task 1's identity-mismatch test.
- Heartbeat cannot masquerade as a sequence-owned mutation; pin this in Task 1's heartbeat-with-event-sequence rejection test.
- Unsupported node status values, including `offline`, are rejected; pin this in Task 1's status enum tests.
- Alert state and removal are mutually exclusive, and invalid lifecycle values are rejected; pin this in Task 1's alert state/removal tests.
- Empty/oversized summaries and unknown/secret-bearing payload fields are rejected; pin this in Task 1's alert bounds/closed-field tests.

---

### Task 1: Typed Rust payloads and validation

**Files:**
- Create: `crates/auxp/src/node.rs`
- Modify: `crates/auxp/src/lib.rs`
- Modify: `crates/auxp/src/message.rs`

**Interfaces:**
- Produce public `NodeStatus` (`Ready`, `Draining`, `Error`), `NodeState { node_id: String, status: NodeStatus, capacity: serde_json::Value }`, `AlertStatus` (`Active`, `Resolved`), `NodeAlertState { alert_id, producer, code, condition_key, summary, first_observed_at, last_observed_at, status }`, and `NodeAlertEvent` (tagged by `kind`, with `State { alert: NodeAlertState }` and `Removed { alert_id: String }` variants).
- `Params` exposes `node: Option<NodeState>` for `node.status_changed` and `alert: Option<NodeAlertEvent>` for `node.alert`.
- `validate` requires `eventId`, `sequence`, and the corresponding typed payload for these two event methods. Update the method/schema required-parameter catalogs together in Task 2 because the contract test requires them to match.

- [x] **Step 1: Add failing Rust unit tests**

Test valid status values, envelope/payload node identity match, alert state and removal deserialization, `lastObservedAt >= firstObservedAt`, heartbeat rejection when it includes `eventId` or `sequence`, unknown fields, invalid statuses, identity mismatch, malformed removal, an empty summary, and a summary longer than 2,000 Unicode characters.

- [x] **Step 2: Run the focused tests and confirm they fail for missing types/validation**

Run: `cargo test -p auxp message::tests`
Expected: FAIL because the typed event payload types and method-specific checks are not implemented.

- [x] **Step 3: Implement typed payloads and method-specific validation**

In `node.rs`, use Serde camelCase fields, closed payload structs, a `kind`-tagged alert event (`state` or `removed`), an object-valued capacity, and RFC3339 timestamps. In `message.rs`, deserialize each event payload, check payload `nodeId` against envelope `nodeId`, validate timestamp order and summary length, and prohibit heartbeat event IDs/sequences. Update `Params` and `lib.rs`; keep generic method metadata and unrelated validation unchanged until Task 2.

- [x] **Step 4: Run focused tests and confirm they pass**

Run: `cargo test -p auxp message::tests`
Expected: PASS, including the five Review Focus input classes.

- [x] **Step 5: Commit the Rust contract types and validation**

```bash
git add crates/auxp/src/node.rs crates/auxp/src/lib.rs crates/auxp/src/message.rs
git commit -m "feat: validate typed node status and alert events"
```

### Task 2: JSON Schemas and conformance fixtures

**Files:**
- Modify: `schemas/v1/messages.schema.json`
- Modify: `schemas/v1/node-snapshot.schema.json`
- Modify: `crates/auxp/src/methods.rs`
- Modify: `fixtures/v1/messages.json`
- Modify: `fixtures/v1/node-snapshot.json`
- Modify: `scripts/validate-fixtures.mjs`

**Interfaces:**
- Consume Task 1's method names and payload shapes.
- Produce JSON Schema definitions for `nodeState`, `nodeAlertState`, and the tagged alert event; snapshot node and alert state must use the same enums and required current-state fields.

- [x] **Step 1: Add valid and invalid message and snapshot fixtures**

Add valid `node.status_changed`, `node.alert` active/resolved/removal, and unsequenced heartbeat examples. Add invalid cases for missing node capacity, envelope identity mismatch, `offline` as operational status, heartbeat with a sequence, invalid alert status/removal, empty or oversized summary, and unknown secret-bearing fields. Add invalid snapshot page chains for an unsupported node status and an incomplete alert `currentState`. Run `node scripts/validate-fixtures.mjs` and confirm it fails because its current validator accepts at least one new invalid event and snapshot.

- [x] **Step 2: Implement schema definitions and update snapshot alignment**

In `messages.schema.json`, require `node` on `node.status_changed` and `alert` on `node.alert`, with closed typed shapes and status enums. Update `methods::required_params` to require the same fields, keeping the catalog contract test aligned. In `node-snapshot.schema.json`, constrain node status and capacity, require the complete alert current-state shape, and accept only `active`/`resolved`. Update the snapshot fixture from `open` to `active` and supply the full alert record.

- [x] **Step 3: Make the JavaScript fixture validator enforce the same event and snapshot invariants**

In `scripts/validate-fixtures.mjs`, check method-specific payloads, heartbeat's lack of event sequence, matching node IDs, valid enums, non-empty summary, summary bounds, and tagged alert state/removal shape. Extend snapshot page validation to reject unsupported node status/capacity and incomplete alert current state. Keep existing validation behavior intact.

- [x] **Step 4: Run fixture and schema checks**

Run: `cargo test -p auxp --test fixtures`
Expected: PASS for valid messages and snapshot chains; all invalid examples are rejected.

Run: `node scripts/validate-fixtures.mjs`
Expected: PASS with valid fixtures accepted and invalid fixtures rejected.

- [x] **Step 5: Commit schemas and fixtures**

```bash
git add schemas/v1/messages.schema.json schemas/v1/node-snapshot.schema.json crates/auxp/src/methods.rs fixtures/v1/messages.json fixtures/v1/node-snapshot.json scripts/validate-fixtures.mjs
git commit -m "feat: align node snapshot and event schemas"
```

### Task 3: Protocol documentation and full verification

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consume the finalized schema and fixture contract from Tasks 1–2.
- Document operational node status separately from heartbeat-derived fleet presence, and explain the alert lifecycle and tombstone sequence.

- [x] **Step 1: Update the event catalog and recovery guidance**

Replace the ambiguous `online/offline/draining/error` event description with `ready/draining/error` operational state. Explain that status/capacity events replace the complete state, heartbeat repeats the last committed values without changing sequence-owned state, and alert state/removal follows the typed schema.

- [x] **Step 2: Run the complete repository CI commands**

Run: `cargo fmt --all --check`
Expected: PASS.

Run: `cargo clippy --all-targets --locked -- -D warnings`
Expected: PASS with no warnings.

Run: `cargo test --locked`
Expected: PASS for all unit, fixture, and snapshot tests.

Run: `node scripts/validate-fixtures.mjs`
Expected: PASS.

- [x] **Step 3: Commit protocol documentation**

```bash
git add README.md
git commit -m "docs: clarify sequenced node and alert state"
```
