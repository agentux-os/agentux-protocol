# ADR 0016: Atomic node snapshot checkpoints

- **Status:** Accepted
- **Date:** 2026-11-04
- **Issue:** [agentux-protocol#16](https://github.com/agentux-os/agentux-protocol/issues/16)

## Context

When an event cursor expires, per-run `run.snapshot` cannot discover runs created while disconnected, provide one node-wide sequence boundary, or recover missing historical events. The receiver must repair its current-state projection without advancing its cursor over uncovered state or fabricating history.

## Decision

Add the optional `node.snapshot_checkpoint.v1` capability and the Command-to-node `node.snapshot` method in AUXP v1. It is dependent on mutual negotiation of `node.event_replay.v1`. One request establishes an immutable, authenticated organization/node-scoped snapshot and subsequent requests page through that same snapshot. Command supplies a stable `checkpointId`; each page repeats a stable `snapshotId`, starting cursor, high-water sequence, exact projection coverage, and page/entry manifest. The concatenated entries form complete authoritative inventories for every current-state projection updated by the node event stream: approval, artifact, command, gate, harness, limit, node, node alert, run, session and task. Historical event feeds remain explicitly gapped and are never synthesized.

The snapshot source is the node's durable, materialized AUXP projections and ownership ledger, captured at the same publication barrier that assigns event sequences. Command-owned run inventory is scoped by authenticated organization and command ownership; unowned local-only runs are excluded. Entries have stable, projection-prefixed IDs; run removals carry `runId` and `removedAtSequence`. Full-set reconciliation makes compacted tombstones safe after the retention period and all pinned snapshots expire.

The receiver gathers all raw page bytes and validates the entire chain through the shared `auxp::snapshot::validate_page_chain` API before any state write. It commits the complete replacement of covered node-owned projections, a history-gap marker, and `highWaterSequence` in one local durable transaction. It only then sends `node.events.ack` with `acknowledgementType: "snapshot_checkpoint"`, a required `checkpointId`, and `throughSequence` exactly at the high-water sequence. The node accepts this ACK only after serving the complete chain and only for the matching identity, checkpoint, starting cursor (still equal to the node's last accepted ACK), and high-water mark. Post-checkpoint events remain replayable and require ordinary contiguous application before a later event ACK. No historical event or activity is synthesized from snapshot data.

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
- If a node cannot provide every required projection at one consistent event-sequence boundary, it returns `snapshot_unavailable` and cannot issue a checkpoint ACK.

## Decision record

The Protocol owner accepts the immutable paged checkpoint contract, the limits, the eleven-projection inventory, authenticated organization and Command ownership scope, tombstone retention/compaction rules, checkpoint ACK binding, and the `node.snapshot_checkpoint.v1` capability / `node.snapshot` method names. Implementations must preserve the all-pages validation and atomic persistence requirements above. Any future projection added to the sequenced node event stream must also be added to checkpoint coverage before a checkpoint ACK can cover events that update it.
