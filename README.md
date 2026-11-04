# AgentUX Protocol

<p align="center">
  <a href="https://agentuxos.org">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/agentux-os/agentux/main/brand/svg/lockup-dark.svg">
      <img alt="AgentUX" src="https://raw.githubusercontent.com/agentux-os/agentux/main/brand/svg/lockup-light.svg" width="320">
    </picture>
  </a>
</p>

<p>
  <a href="https://github.com/agentux-os/agentux-protocol/commits/main"><img alt="Last commit" src="https://img.shields.io/github/last-commit/agentux-os/agentux-protocol?label=updated&labelColor=0b0d10&color=4f7d0b"></a>
  <a href="schemas/v1/messages.schema.json"><img alt="Protocol version" src="https://img.shields.io/badge/protocol-AUXP%20v1-4f7d0b?labelColor=0b0d10"></a>
  <a href="https://github.com/agentux-os/agentux-command"><img alt="Command integration" src="https://img.shields.io/badge/integrates%20with-AgentUX%20Command-c6f36b?labelColor=0b0d10"></a>
</p>

AgentUX Protocol (AUXP) is the versioned remote contract between AgentUX Command and AgentUX nodes.

The contract is maintained from the schemas and fixtures in this repository. Compatibility is explicit: additive changes stay in `v1`; breaking semantic changes require a new major protocol version. The README intentionally points to the live schema instead of duplicating a version number that would need manual updates.

It lets a remote control plane coordinate multiple local AgentUX installations without accessing their local sockets, databases, credentials or worktrees directly.

## Boundary

```text
AgentUX Command
  owns organizations, missions, placement, governance and global telemetry
                          │ AUXP over TLS
                          ▼
AgentUX Node
  owns agentuxd, worktrees, ACP sessions, local approvals and technical runs
```

Command is authoritative for the organizational mission. The node is authoritative for the technical execution. The Office UI is only a projection of both state streams.

## Transport

- WebSocket over TLS, initiated outbound by the node.
- JSON-RPC 2.0 messages over the WebSocket.
- JSON objects use `camelCase`; enum values use `snake_case`.
- Protocol version is negotiated during `node.hello`.
- Commands are request/response messages. Events are JSON-RPC notifications.

The node must not expose the local `agentuxd` Unix socket to the network.

**Connections.** The node opens `wss://<command>/v1/protocol/connect?nodeId=<id>` with `Authorization: Bearer <node token>`. Over it Command sends command requests and the node answers each with a JSON-RPC response (`result`, or `error` with `code` and `message`). Node events may go over the same socket or be POSTed to `/v1/protocol/messages`; Command accepts both. Error codes: `-32601` the node does not implement the method, `-32602` invalid params, `-32010` held for local confirmation, `-32011` denied by the node's policy, `-32012` the local daemon refused or failed.

**`run.start` on a node.** `repository` is the git URL of a project the node has cloned, or clones (into `~/src`). `roles` names the roles the mission expects; optional `agents` (role → harness) and `models` (role → model) choose them, within the node's own agents. `prompt`, optional `title` and `budget.maxUsd` complete it. `isolationPolicy: "worktree"` requests an isolated Git worktree for the project; it does not select the agent-session sandbox. The node's local policy remains authoritative for sandbox enforcement, so AUXP v1 does not let `run.start` weaken or independently override that policy.

## Identity and correlation

Every message that changes or reports state carries:

- `protocolVersion`
- `organizationId`
- `nodeId`
- `messageId`
- `occurredAt`

Run-related messages also carry `missionId`, `runId` and, when applicable, `sessionId`. Commands carry an idempotency key in `commandId`; events carry a monotonic `sequence` per node stream.

## Node lifecycle

1. Node opens an outbound TLS connection.
2. Node sends `node.hello` with identity, protocol versions and capabilities.
3. Command responds with the accepted version and a connection lease.
4. Node sends `node.heartbeat` at the negotiated interval.
5. Node sends events; Command acknowledges only after durable application.
6. On reconnect, Command invokes `node.resume` with its last durably applied sequence; the node replays its retained notifications to Command.

## Enrollment and sessions

The contract is [`schemas/v1/enrollment.schema.json`](schemas/v1/enrollment.schema.json), with examples in [`fixtures/v1/enrollment.json`](fixtures/v1/enrollment.json).

**Identity.** Every Command server has an instance id (`cmd-` and 16 hex digits), created once and kept for its database's life. `GET /v1/instance` answers `{ product, instanceId, protocolVersions }`, unauthenticated and with no tenant data.

**Enrollment.** An operator issues enrollment tokens (`auxe1_<instanceId>_<64 hex>`) for one organization. A token names its server, is single use, expires (24 hours by default, 30 days at most), may be for one node name only, and may require an operator's approval. The server keeps only its hash and shows it once. A node:

1. reads the instance id from the token and compares it with `GET /v1/instance` at the address it was given, and stops on a mismatch **before sending the token**;
2. redeems it with `POST /v1/enroll` `{ token, nodeName?, hostname? }` and gets `{ instanceId, organizationId, nodeId, nodeSecret, approved }`. The node id is unique on that server: a suffix is added when a name is taken, and never someone else's id is reused. A refusal is 401 (unknown, used or expired token) or 409 (another server's token, answered with this server's `instanceId`, or a node name already enrolled);
3. keeps the credential (`auxn1_<64 hex>`) readable by its user only, and **pins** the instance id.

**Credentials.** A node credential is bound to its node id and organization on one server. It is returned once, by the redemption, and never in any other answer, event or log line. Every node request carries it as `Authorization: Bearer`. Messages for another node or organization are refused (401).

**Approval.** A node enrolled with a token that requires approval may send `node.hello` and `node.heartbeat` only (it shows as `pending_approval`). Anything else answers 403 until an operator approves it.

**Pinning.** A pinned node checks the instance on every answer that names one (`result.instanceId` of its messages), on the command connection (`X-AgentUX-Command-Instance` response header) and in every command (`commandInstanceId`). It refuses commands from any other instance and stops reporting to a server that answers as another.

**Revocation and rotation.** Revoking a node invalidates its credential at once and closes its command connection; its messages answer 401. Credentials do not expire on their own. Rotation is re-enrollment: an operator revokes the node and issues a new token, and the node enrolls again, keeping its node id when the token names it.

**Replay protection.** Tokens are single use and expiring. Commands are idempotent by `commandId`: a repeated command gets the first answer. Events are applied once per `(nodeId, sequence)`. A command carries the issuing server's `commandInstanceId`, so a command captured from one server is refused by nodes of another. Remote connections use TLS; plain HTTP is for the node's own machine only.

**Sessions and reconnect.** The node opens the command connection (`/v1/protocol/connect?nodeId=`, WebSocket, its credential) and keeps it with backoff; a new connection replaces the node's previous one. Events wait in the node's ordered outbox while Command is unreachable and are sent again in order on reconnect; duplicates are ignored by `(nodeId, sequence)` and `eventId`. With `node.event_replay.v1` negotiated, Command's `node.resume.lastAcknowledgedSequence` acknowledges through N inclusively and asks the node to resend only sequences greater than N. Replay is strictly ascending and contains every retained sequence through a declared high-water mark. Consumers apply and persist events idempotently, then acknowledge only the highest contiguous sequence durably applied with `node.events.ack`; they never advance across a gap. If the cursor predates retention, the response is `cursor_expired` with `snapshotRequired: true`; the consumer obtains `run.snapshot`, replaces its projection, then resumes from the snapshot's declared sequence. Repeating a resume request is safe and may return duplicates. An accepted run never depends on the connection.

**Versions.** `node.hello` lists `supportedProtocolVersions`, which must contain `v1`. A message with any other `protocolVersion` is refused (400, `unsupported protocol version`), and so is a method a server does not know. A node answers a command it does not implement with `-32601`.

## Commands

| Method | Direction | Purpose |
|---|---|---|
| `node.hello` | node → command | Authenticate and negotiate capabilities |
| `node.heartbeat` | node → command | Report liveness and capacity |
| `node.resume` | command → node | Ask the node to replay retained event notifications after Command's cursor |
| `node.events.ack` | command → node | Acknowledge Command's highest contiguous, durably applied event sequence |
| `run.start` | command → node | Start a technical run for a mission |
| `run.cancel` | command → node | Cancel a run |
| `run.pause` | command → node | Pause at the next safe boundary |
| `run.resume` | command → node | Resume a paused run |
| `request.resolve` | command → node | Resolve a human approval/question |
| `run.snapshot` | command → node | Request current local state |
| `bus.post` | command → node | Deliver an authorized human message to a run |
| `harness.list` | command → node | The node's agent CLIs: installed, signed in, version, spend, plan usage, usage limits |
| `harness.test` | command → node | Send one agent a short test prompt; the result says whether it answered |
| `limits.clear` | command → node | An operator lifted a usage limit (credits added, plan reset) |
| `preferences.set` | command → node | Replace the node's agent preferences (order, role agents and models, fallbacks, stall minutes, notifications) |

`run.start` must include a complete execution request: repository, prompt/issue, pipeline reference, roles, budget, workspace isolation policy and requested capabilities. It must not include provider credentials. Agent-session sandbox requirements are enforced by the node policy; an organization may require a sandbox for Command-originated runs, but cannot disable a node's requirement.

## Events

| Event | Meaning |
|---|---|
| `node.status_changed` | Node online/offline/draining/error |
| `run.accepted` | Node accepted a run command |
| `run.started` | Worktree and technical run exist |
| `run.updated` | Step, cost or activity changed |
| `session.event` | ACP-normalized session event |
| `approval.requested` | Human decision is required |
| `gate.completed` | Checks completed with results |
| `run.completed` | Technical run finished |
| `run.failed` | Technical run failed |
| `artifact.available` | Log, diff, plan or report is available |
| `node.alert` | Local safety, capacity or process issue |
| `harness.status` | The node's agent CLIs changed (same shape as `harness.list`'s result) |
| `harness.limits_changed` | Usage limits in force changed: every limit still in force |
| `command.denied` | The node's policy refused a command, or holds it for local confirmation (`reason`) |
| `command.completed` | A command held for confirmation ran (or was rejected) at the node: `commandId`, `method`, `ok`, `result` or `error`, `confirmedBy` |

After the first agent session opens, `run.updated`, `run.completed` and `run.failed` report the run's effective `agentIsolation` when available. It describes the weakest isolation used by any agent session in that run: host mode (`none`) is weaker than `bubblewrap`, and sandbox network access (`outbound`) is weaker than no network (`none`). A host run omits `network`. `source` is selected from the session(s) establishing that weakest mode/network; if they tie, use the highest-precedence source in this order: `node-policy`, `project`, `command`, `default`. The field can be absent before a session starts. Consumers should treat it as observed execution state, not as a request or a promise that future sessions will be more restricted.

Events are at-least-once. Consumers deduplicate by `eventId`, order by `(nodeId, sequence)`, and persist the last acknowledged sequence. When `node.event_replay.v1` is not negotiated, peers do not invoke `node.resume` or `node.events.ack`; they retain the existing live/outbox delivery behavior and legacy event semantics.

### Typed execution activity (optional v1 capability)

The node offers capabilities in `node.hello.capabilities`; Command returns the accepted subset as `acceptedCapabilities` in its hello result. An extension is active only when Command accepts the node's offer. Older Command servers that omit `acceptedCapabilities` negotiate no optional extension; peers continue with existing v1 methods and event meanings. `execution.activity.v1` gates `execution.activity`; `node.event_replay.v1` gates the replay direction and acknowledgement commands. The contract is [`schemas/v1/execution-activity.schema.json`](schemas/v1/execution-activity.schema.json), referenced by the `execution.activity` notification in [`schemas/v1/messages.schema.json`](schemas/v1/messages.schema.json).

The activity envelope carries `schemaVersion`, stable `activityId`, mission/run, agent/role and optional session, task/parent-task, attempt, causation, delegation and tool-call identifiers. Its discriminated `type` represents plan/task creation and status, tool start/progress/completion/failure, assignment/delegation/handoff/result return, task completion/failure, or artifact availability. Task and delegation IDs remain stable across events; each retry increments `attempt`, each derived event names its `causationId`, and delegation events use one stable `delegationId` across handoff and returned result. A consumer builds the task tree by `(missionId, runId, taskId)` and links delegation edges by `delegationId`; node-local `sequence` supplies total order within one node, while causation links order related events across nodes. There is no implied total order between unrelated node streams.

### Atomic node snapshot checkpoint (proposed optional v1 capability)

`node.snapshot_checkpoint.v1` is an optional extension. The node offers it in `node.hello.capabilities`; Command activates it only when it echoes the capability in `acceptedCapabilities`. If it is absent or not accepted, peers retain the existing per-run `run.snapshot` behavior and must not call `node.snapshot`; a legacy peer must never infer that it has checkpoint semantics.

When `node.resume` reports `cursor_expired`, Command may call `node.snapshot` with a fresh `checkpointId`, the exact expired `requestedAfterSequence`, a `pageSize` from 1 through 100, and optionally an opaque `pageToken`. The node requires the starting cursor to match the authenticated Command cursor and rejects a stale value. The first request captures one immutable, authenticated organization/node-wide point-in-time view at a single linearization point: every state-changing event through `highWaterSequence` is reflected in the snapshot, and every later event has a sequence greater than it. Every page reports the same `schemaVersion: "v1"`, `checkpointId`, `snapshotId`, scope, `requestedAfterSequence`, `highWaterSequence`, `pageCount` and `entryCount`. The view includes every run record in the node's authoritative inventory and explicit `removed` entries for known removals; entries are unique and sorted by `entryId` across the concatenated pages. An empty inventory is one empty terminal page. Run records include the complete current state for that run; no historical transitions are implied.

Pages form one snapshot, not independent reads. Command follows each returned `nextPageToken`, verifies the immutable manifest, page indexes, unique ordering, entry count and final-page boundary, and discards the entire attempt if a page is missing, inconsistent, oversized or expired. A page is at most 100 entries and 1 MiB encoded; a node MUST return fewer entries when needed to meet the byte limit. If one entry alone exceeds the limit, return `snapshot_entry_too_large` and do not issue a usable checkpoint. A snapshot contains at most 10,000 entries and 64 MiB encoded. If those limits are exceeded, return `snapshot_too_large` without a partial checkpoint. Page tokens are opaque, bound to the authenticated scope and snapshot, and stable on retry. The snapshot remains available for at least 15 minutes; an expired snapshot returns `snapshot_expired`, and recovery restarts with a new `checkpointId`.

`checkpointId` is generated by Command and stable for the recovery attempt. Reusing it with the same request identity returns the same snapshot and page contents; changing `requestedAfterSequence` or scope with the same ID is `checkpoint_conflict`. Each JSON-RPC page request has its own `commandId`; retrying a request with the same `commandId` returns the original page, while a new page token uses a new `commandId`. A repeated complete attempt after reconnect is safe and yields the same checkpoint during its retention window. JSON-RPC error `data` for this method uses `schemas/v1/node-snapshot.schema.json#/$defs/failure`: `snapshot_expired`, `snapshot_too_large`, `snapshot_entry_too_large`, `checkpoint_conflict`, or `stale_checkpoint`. Checkpoint ACK refusal reports `checkpoint_incomplete`, `checkpoint_scope_mismatch`, or `checkpoint_sequence_mismatch`. Every error leaves both cursors unchanged; restart after expiry or size failure with a new checkpoint ID and do not use a partial response.

Command MUST apply all pages as one durable local transaction: replace the node's current run inventory (discovering new runs, applying current states and removals), persist a history-gap marker for `(requestedAfterSequence, highWaterSequence]`, and set its contiguous cursor to `highWaterSequence`. Commit none of these if any page or validation fails. Only after that transaction commits may Command send `node.events.ack` with `throughSequence` equal to the checkpoint's `highWaterSequence` and the matching `checkpointId`. The node accepts that checkpoint ACK only for the authenticated organization/node, an issued unexpired checkpoint with every page served, the exact `highWaterSequence`, and the recorded starting cursor; otherwise it rejects the ACK and leaves the cursor unchanged. Ordinary ACKs still require contiguous durable event application. Events with sequence greater than the checkpoint high-water remain available for ordinary replay and must be applied contiguously before a later ACK.

This checkpoint repairs current state and the replay cursor. It does not recreate the expired interval's event history. Command records that interval as a history gap and MUST NOT synthesize timeline, activity, approval, audit, or other event records from snapshot state. Consumers may display current run state separately from historical event feeds. The contract and examples are in [`schemas/v1/node-snapshot.schema.json`](schemas/v1/node-snapshot.schema.json) and [`fixtures/v1/node-snapshot.json`](fixtures/v1/node-snapshot.json); its ADR remains Proposed pending owner decision.

Summaries are operator-safe, at most 2,000 Unicode characters. Do not emit private chain-of-thought, hidden reasoning, prompts, credentials, environment values, raw tool inputs or outputs, or arbitrary provider payloads. Artifact content stays out of band: references require an HTTPS or `artifact://` URI, SHA-256 digest, media type and byte length; each reference is at most 25 MiB and each event carries at most 16 references. Implementations must enforce authorization and access controls when dereferencing artifacts. Unknown activity fields and variants are rejected; evolve the shape with a new schema version/capability before sending it.

### `node.resume` replay contract

Replay is gated by `node.event_replay.v1`, negotiated in `node.hello.capabilities`, because it assigns the existing `node.resume` method a Command → node direction. Command invokes it with `commandId` and `lastAcknowledgedSequence`; the cursor is inclusive as an acknowledgement (“durably applied through N”) and exclusive as a replay boundary (the node resends only sequence `> N`). The node resends original notifications to Command on the event channel in strict ascending order and returns only replay metadata in the JSON-RPC result. It never returns its own notifications to itself. A repeated `commandId` must replay the same requested range; using a different cursor requires a new `commandId`. `node.events.ack` carries Command's highest contiguous sequence only after durable application; lower or repeated acknowledgements do not move the cursor backward. Both methods are used only when the replay capability is negotiated.

The `node.resume` result shape is `schemas/v1/messages.schema.json#/$defs/resumeResult` and Rust's `auxp::replay::ReplayResult`; it reports `status`, `nodeId`, `requestedAfterSequence`, `oldestAvailableSequence`, `highWaterSequence`, `eventsReplayed`, and `snapshotRequired`. An empty replay is valid when the cursor equals the high-water mark. `cursor_expired` is required when the cursor is before the retained range; it has `snapshotRequired: true` and `eventsReplayed: 0`. On that result, Command invokes `run.snapshot` on the same node; the node returns its current snapshot in that RPC result, and Command replaces its projection before resuming from the snapshot's declared sequence. Duplicate delivery is expected: deduplicate by `(nodeId, sequence)` and verify the same `eventId` maps to the same sequence. A sequence gap or conflicting event identity stops projection advancement; do not silently skip or infer missing events. A sender retains unacknowledged events within its configured retention window.

## Node policy

Every node decides what Command may do on it, in `/etc/agentux/node-policy.yaml`. Each command method is `allow`, `confirm` (held until someone at the node accepts it in the cockpit or with `aux`) or `deny`. The default policy:

| Method | Default |
|---|---|
| `run.snapshot`, `harness.list`, `harness.test` | `allow` |
| `node.resume`, `node.events.ack` | `allow` |
| `run.cancel`, `run.pause`, `run.resume`, `request.resolve`, `bus.post`, `limits.clear` | `allow` |
| `run.start`, `preferences.set` | `confirm` |

A held or refused command answers with a JSON-RPC error (`-32010` held, `-32011` denied) and a `command.denied` event carrying `commandId`, `method` and `reason` (`awaiting_local_confirmation`, `denied_by_policy`). When someone at the node accepts or rejects a held command, a `command.completed` event reports it under the original `commandId`. A command sent again with the same `commandId` returns the first answer. There is no method that runs a shell command or carries credentials; API keys stay in the node's own secret store.

## Reliability rules

- Commands are idempotent by `commandId`.
- A duplicated command returns the original result.
- Events may be duplicated and must be safe to replay.
- The node continues an accepted run if Command disconnects.
- The node buffers events locally while disconnected.
- Command never assumes that a timeout means the run failed.
- A run is only terminal after an explicit `run.completed`, `run.failed` or `run.cancelled` event.

## Security rules

- TLS is mandatory for remote connections.
- Nodes use scoped, rotatable credentials; credentials are never sent in `run.start`.
- Command enforces organization and role authorization before issuing commands.
- Nodes validate command scope against their local policy before execution.
- All commands, approvals and security denials are audited.
- Raw provider tokens, local environment variables and unrestricted shell commands are never protocol payloads.

## Versioning

The first public contract is `v1`. Additive fields are allowed. Removing or changing field meaning requires a new major protocol version. Capabilities allow a Command server and node to degrade gracefully when optional methods are unavailable.

## Initial implementation order

1. Read-only `node.hello`, heartbeat and `node.status_changed`.
2. Run snapshot, durable per-node event cursors, and `node.resume` replay/expiry handling.
3. `run.start` and terminal run events.
4. Remote approvals and cancellation.
5. Pause/resume, bus messages and bounded artifact references.
6. **agentux-core follow-up:** emit capability-gated typed activity from execution and delegation lifecycles, redact summaries, assign stable IDs/sequences, bound artifact references, retain/replay the event outbox, and acknowledge Command's contiguous durable cursor.
7. **agentux-command follow-up:** negotiate capability, validate and persist typed events idempotently by `(nodeId, sequence)`/`eventId`, detect gaps/expiry, serve resume replay, acknowledge durable progress, and project mission/task/delegation trees with snapshot recovery.

## Schemas and fixtures

The v1 message contract is in [`schemas/v1/messages.schema.json`](schemas/v1/messages.schema.json), with representative messages in [`fixtures/v1/messages.json`](fixtures/v1/messages.json). Run the dependency-free contract check with:

```sh
node scripts/validate-fixtures.mjs
```

The Office projection contract is defined in [`schemas/v1/office-projection.schema.json`](schemas/v1/office-projection.schema.json), with approval resolution in [`schemas/v1/approval-resolution.schema.json`](schemas/v1/approval-resolution.schema.json). Approval requests include `organizationId`; Command scopes idempotency by `(organizationId, approvalId, idempotencyKey)` and echoes the organization in successful responses. The event log remains at-least-once: deduplicate by `eventId`, order by `(nodeId, sequence)` and resume from each node's acknowledged cursor.

## Rust: the `auxp` crate

[`crates/auxp`](crates/auxp) is AUXP for Rust: the methods and the params each requires, message validation, error codes, the node policy defaults, enrollment token and credential formats, and the provision file. The node gateway (agentux-core) and the Command server (agentux-command) both depend on it, so the two sides cannot drift apart. Its tests run against this repository's schemas and fixtures: a schema change that the crate does not follow fails CI.

```toml
auxp = { git = "https://github.com/agentux-os/agentux-protocol", rev = "<commit>" }
```

```sh
cargo test        # the crate, against schemas/ and fixtures/
node scripts/validate-fixtures.mjs
```
