# ADR 0020: Authenticated artifact retrieval and lifecycle

- **Status:** Proposed
- **Date:** 2026-11-06
- **Issue:** [agentux-protocol#20](https://github.com/agentux-os/agentux-protocol/issues/20)

## Context

Protocol #12 carries a bounded `ArtifactRef` and `artifact_available` activity,
but has no content retrieval method or removal activity. Protocol #17 can
recover current state after event replay expires, but its generic artifact
projection does not identify a content revision. Core #118 proposes a
node-local content store and requires exact digest-bound retrieval, truthful
producer identity, complete checkpoint state, and bounded retention.

Adding a mutable URL or a caller-supplied host path would leave old events
pointing at different bytes, expose credentials or the node filesystem, and
make access control depend on the producer. The contract must work over the
existing authenticated node connection, retain the exact bytes during replay,
and keep older peers on their existing behavior.

## Decision

Add optional capability `artifact.lifecycle.v1`. It is usable only when
Command accepts it together with `execution.activity.v1`,
`node.event_replay.v1`, and `node.snapshot_checkpoint.v1`. The AUXP crate's
negotiated validation entry point rejects `artifact.get` and
`artifact_removed` unless all four appear in the accepted set; transports must
call it before dispatching an optional extension. A node must not send
the new removal event, serve artifact chunks, or claim the strict artifact
checkpoint profile unless all four capabilities are accepted.

### Immutable references and retrieval

- Under this capability, an `ArtifactRef.uri` uses the node-local form
  `artifact://<nodeId>/<artifactId>/<sha256>`. It identifies one immutable
  content revision; `latest` aliases and HTTPS URLs are not used by this
  profile.
- Add Command-to-node request `artifact.get`, scoped by the authenticated
  organization and node connection. It carries `commandId`, `artifactId`, the
  lowercase SHA-256, byte `offset`, and `maxBytes` (1..=262144). It carries no
  URI, host path, or credential.
- The result carries `artifactId`, `sha256`, `mediaType`, total `byteLength`,
  the requested `offset`, and `dataBase64`. Decoded bytes are at most 256 KiB,
  cannot extend past `byteLength`, and the tuple must match the request. A
  zero-byte artifact returns one empty chunk at offset zero.
- Each request is idempotent by its JSON-RPC `commandId`. A next chunk uses a
  new command ID and the previous offset plus decoded chunk length. The client
  verifies the final total length and SHA-256 before exposing the file.
- The node authorizes every chunk against the authenticated organization and
  the artifact's persisted run ownership. Unknown IDs, wrong digests,
  cross-organization access, expired content, and invalid ranges fail without
  returning bytes. Authentication is inherited from the node connection; no
  token or signed secret is embedded in an activity or URI.

### Lifecycle and replay

- `artifact_available` creates or updates the current logical artifact. An
  update keeps the active `artifactId`, publishes the new immutable URI/digest,
  and advances the event sequence.
- Add `artifact_removed` as an execution activity variant, carrying the
  `artifactId`. The `agentId` and `roleId` identify the original producer,
  including for a Core cleanup event; Core/system output without truthful
  agent attribution is not published under this capability.
- `artifactId` is never reused after removal. Re-publishing the same producer
  key creates a new ID, so a stale tombstone cannot remove a later artifact.
- Removal also produces the Protocol #17 projection tombstone at its exact
  sequence. A full checkpoint remains authoritative for artifact inventory;
  it is not a synthesized activity event.
- Superseded and removed content remains retrievable for at least 30 days and
  longer while a replay/outbox entry, unexpired checkpoint, or active download
  references it. After all holds expire, dereference returns `410 Gone` and
  never maps the old URI to other bytes. Tombstones follow the Protocol #17
  retention and pinned-snapshot rules.

### Checkpoint profile

The existing Protocol #17 artifact shape remains valid for peers that do not
negotiate `artifact.lifecycle.v1`. With the new capability, every live
artifact checkpoint state must additionally include `runId`, `revision`
(lowercase SHA-256), `mediaType`, `byteLength`, and the exact immutable `uri`,
alongside `artifactId` and `organizationId`. Core and Command validate this
profile before materialization or projection replacement. A missing field or
unresolvable revision makes the whole checkpoint invalid and cannot advance
the cursor. Removed entries retain the existing `removedAtSequence`
tombstone shape.

### Bounds and safe presentation

The existing 25 MiB per-reference and 16 references per activity limits
remain. A fetch response contains at most 256 KiB of decoded bytes. Receivers
verify the digest and byte length and treat content as an attachment by
default; active content is not rendered inline by this protocol.

## Alternatives considered

1. **Fetch an arbitrary HTTPS URI.** Rejected for this profile: the node cannot
   safely authorize redirects, prevent SSRF, or avoid forwarding credentials.
2. **Send all bytes in one JSON-RPC response.** Rejected because one response
   can approach the 25 MiB artifact limit and exceed WebSocket/server message
   bounds. Bounded range chunks keep memory and retry costs predictable.
3. **Treat `artifact_available` as sufficient for removal and updates.**
   Rejected because it cannot express deletion and a mutable latest URI would
   make old events resolve to different content.
4. **Tighten every existing Protocol #17 artifact entry.** Rejected because
   peers already using the accepted checkpoint contract would become invalid.
   The new profile is negotiated independently and leaves their schema valid.

## Consequences

- Nodes need a digest-addressed store and authorized chunk reader; Command needs
  a bounded downloader and digest verification.
- Protocol #17 validation remains backward-compatible, while new-capability
  implementations add strict artifact-state validation.
- Artifact removals become replayable current-state changes and recoverable
  through checkpoint tombstones.
- Core keeps generic artifacts unavailable until it implements this capability
  and proves every configured producer inventory complete.
- The protocol does not publish Core-only/system artifacts without truthful
  agent/role attribution; a future source-identity model would need a separate
  compatible extension.

## Decision record

Pending Protocol owner review. No peer may advertise or use
`artifact.lifecycle.v1` until this decision is accepted and the schemas and
implementations are merged.
