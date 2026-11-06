# Protocol #20: Artifact retrieval and lifecycle

**Goal:** Define and validate authenticated artifact retrieval, exact content
revisions, removal/replay, and checkpoint recovery without breaking peers
that only negotiate existing v1 capabilities.

**Design:** `docs/adr/0020-artifact-retrieval-and-lifecycle.md` (Proposed).

## Constraints

- New behavior is gated by `artifact.lifecycle.v1` plus execution activity,
  replay, and snapshot checkpoint capabilities.
- Fetches are chunked at 256 KiB and bound to `artifactId` plus lowercase
  SHA-256; never fetch arbitrary URLs or paths.
- `artifactId` is never reused after removal; exact old content remains
  available for 30 days minimum and longer while replay/checkpoint/download
  holds remain.
- Existing Protocol #17 artifact checkpoints remain valid without the new
  capability; negotiated peers enforce the complete revision profile.
- The activity's `agentId`/`roleId` are truthful producer provenance. System
  outputs without that attribution stay local.

## Tasks

1. Add request/result schemas and Rust types/validation for `artifact.get` and
   bounded base64 chunks.
2. Add `artifact_removed` to the typed activity schema/Rust enum and define
   the capability-specific artifact snapshot profile.
3. Add fixtures for valid/invalid requests, results, activity events, and
   live/tombstone checkpoint entries. Exercise schema and Rust validators.
4. Update Protocol README and ADR with negotiation, retention, auth, bounds,
   errors, and implementation ownership for Core and Command.
5. Run fixture validation and Rust CI; request independent contract/security
   review before merging. Keep ADR `Proposed` until the Protocol owner accepts.
