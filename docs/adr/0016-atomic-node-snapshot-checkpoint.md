# ADR 0016: Atomic node snapshot checkpoints

- **Status:** Proposed — requires Protocol owner decision
- **Date:** 2026-11-04
- **Issue:** [agentux-protocol#16](https://github.com/agentux-os/agentux-protocol/issues/16)

## Context

When an event cursor expires, per-run `run.snapshot` cannot discover runs created while disconnected, provide one node-wide sequence boundary, or recover missing historical events. The receiver must repair its current-state projection without advancing its cursor over uncovered state or fabricating history.

## Decision proposed

Add the optional `node.snapshot_checkpoint.v1` capability and the Command-to-node `node.snapshot` method in AUXP v1. One request establishes an immutable, authenticated organization/node-scoped snapshot and subsequent requests page through that same snapshot. Command supplies a stable `checkpointId`; each page repeats a stable `snapshotId`, starting cursor, high-water sequence and page/entry manifest. The concatenated, ordered entries form the complete authoritative run inventory and include explicit removal tombstones. Snapshot and retry constraints are defined in the README and `schemas/v1/node-snapshot.schema.json`.

Command commits the complete replacement projection, a history-gap marker, and `highWaterSequence` in one local durable transaction. It only then sends `node.events.ack` bound to that checkpoint, exactly at its high-water sequence. The node accepts this checkpoint ACK only after it has served the complete snapshot and only for the matching identity, scope, starting cursor and high-water mark. Post-checkpoint events remain replayable and require ordinary contiguous application before a later ACK. No event or activity is synthesized from snapshot data.

The optional capability is negotiated through the existing `node.hello.capabilities` / `acceptedCapabilities` exchange. Without explicit acceptance, existing v1 behavior remains in effect.

## Alternatives considered

1. **One unpaged response.** Simple to implement and offers one atomic read, but a node-wide state response is unbounded and fails as inventories grow. Rejected for the default wire shape.
2. **Independent per-run snapshots plus a final cursor.** Reuses existing `run.snapshot`, but runs can change between reads, new runs remain undiscoverable, and there is no proof that one sequence boundary covers all returned state. Rejected because it cannot meet cursor safety.
3. **Pinned immutable snapshot with bounded pages (recommended).** A single capture defines the state and high-water mark; continuation pages are bounded and retryable, and a manifest lets the receiver reject partial or mixed captures. It requires temporary node-side snapshot retention and receiver-side all-pages validation, so the protocol specifies expiry, size limits and restart semantics.

## Consequences

- Recovery is current-state repair, not historical replay. Consumers must present the missing interval as a gap.
- Command must support atomic durable replacement of a node's run projection, history-gap metadata and cursor.
- Nodes must retain a bounded immutable snapshot during pagination and bind tokens and checkpoint ACKs to the authenticated scope.
- Core and Command can implement independently from the JSON Schema, examples and protocol rules.

## Owner decisions requested

- Accept or revise the recommended immutable paged snapshot design and its proposed limits: 100 entries / 1 MiB per page, 10,000 entries / 64 MiB total, and a 15-minute snapshot lifetime.
- Confirm that an authoritative complete run inventory plus explicit tombstones is the desired removal model.
- Confirm the capability name and `node.snapshot` method name before implementations depend on them.
