# Protocol #18: typed node status and alert events

## Intent

Define the sequenced AUXP v1 event payloads Core needs to persist node status,
capacity, and local alert state at one snapshot high-water mark. Command can
then recover current state without treating an unsequenced heartbeat as durable
history or inventing alerts.

## Goals and constraints

- `node.status_changed` is a full-state replacement for the authenticated node's
  `status` and `capacity`; every change to either value gets a positive event
  sequence. Use `ready`, `draining`, and `error` as operational states.
- `node.heartbeat` repeats the last sequenced status and capacity for liveness
  reporting, but never mutates the durable projection or advances a sequence.
  Command derives fleet presence from heartbeat freshness; an unreachable node
  is not represented by a fabricated `offline` event.
- `node.alert` carries either a full alert state (`active` or `resolved`) or an
  explicit removal tombstone. Alert identity and lifecycle follow Core's
  accepted record: stable `alertId`, producer, code, condition key, bounded
  operator-safe summary, first/last observation timestamps, and status. The
  event envelope's sequence is authoritative for both state changes and
  removals.
- Authenticated organization/node identity remains in the existing message
  envelope. Payloads contain no credentials, environment values, unrestricted
  logs, or arbitrary provider data.
- Snapshot node and alert projections use the same status vocabulary and full
  current-state fields. Update the fixture's legacy alert status `open` to
  `active`.

## Approaches considered

1. **Full-state sequenced events with heartbeat as a non-mutating repeat
   (chosen).** This aligns Core's durable source, replay sequence, and snapshot
   high-water. It keeps fleet presence derived from heartbeat freshness.
2. **Heartbeat as the source of node state.** Rejected because it has no event
   sequence and cannot prove that snapshot status/capacity match the replay
   cursor.
3. **Introduce a new protocol capability/version.** Rejected for this additive
   v1 contract: it would delay the prerequisite without improving compatibility
   over typed payload validation on already sequenced event names.

## Scope

Add JSON Schema definitions and required event payloads, Rust AUXP types and
validation, positive/negative fixtures, and README guidance for event replay,
heartbeat semantics, snapshot alignment, and the alert lifecycle. Keep the
existing JSON-RPC envelope, event names, and event sequence semantics.

## Non-goals

This change does not implement Core alert producers or node persistence, derive
fleet presence in Core, enable `node.snapshot_checkpoint.v1`, or add new alert
fields beyond the Core-owned contract. Snapshot capability stays gated until
Core has authoritative durable sources for every covered projection.

## Implementation calls

The builder should use typed `node` and `alert` payload objects with closed
field sets. A status event carries the complete node state; an alert state event
carries the complete alert record; an alert removal carries only the stable
`alertId` and a removal discriminator, with the outer event sequence serving as
the removal sequence. Snapshot entries must use the equivalent current-state
shape and `kind: removed`/`removedAtSequence` representation. Rust validation
must reject invalid status, alert lifecycle, identity mismatch, missing fields,
and forbidden secret-bearing fields.

## Verification

Fixtures and tests cover each node status, a heartbeat that does not advance
state, alert activation/resolution/removal, invalid IDs/statuses, and a snapshot
whose projection state matches the sequenced event. Run the repository's
schema, fixture, formatting, Clippy, and test checks on CI.
