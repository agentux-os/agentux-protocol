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

**`run.start` on a node.** `repository` is the git URL of a project the node has cloned, or clones (into `~/src`). `roles` names the roles the mission expects; optional `agents` (role → harness) and `models` (role → model) choose them, within the node's own agents. `prompt`, optional `title` and `budget.maxUsd` complete it.

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
5. Node sends events and acknowledges replay cursors.
6. On reconnect, node sends `node.resume` with the last acknowledged sequence.

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

**Sessions and reconnect.** The node opens the command connection (`/v1/protocol/connect?nodeId=`, WebSocket, its credential) and keeps it with backoff; a new connection replaces the node's previous one. Events wait in the node's ordered outbox while Command is unreachable and are sent again in order on reconnect; duplicates are ignored by sequence. `node.resume` (asking Command for missed events) is specified and not implemented yet. An accepted run never depends on the connection.

**Versions.** `node.hello` lists `supportedProtocolVersions`, which must contain `v1`. A message with any other `protocolVersion` is refused (400, `unsupported protocol version`), and so is a method a server does not know. A node answers a command it does not implement with `-32601`.

## Commands

| Method | Direction | Purpose |
|---|---|---|
| `node.hello` | node → command | Authenticate and negotiate capabilities |
| `node.heartbeat` | node → command | Report liveness and capacity |
| `node.resume` | node → command | Request missed events after reconnect |
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

`run.start` must include a complete execution request: repository, prompt/issue, pipeline reference, roles, budget, isolation policy and requested capabilities. It must not include provider credentials.

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

Events are at-least-once. Consumers deduplicate by `eventId`, order by `(nodeId, sequence)`, and persist the last acknowledged sequence.

## Node policy

Every node decides what Command may do on it, in `/etc/agentux/node-policy.yaml`. Each command method is `allow`, `confirm` (held until someone at the node accepts it in the cockpit or with `aux`) or `deny`. The default policy:

| Method | Default |
|---|---|
| `run.snapshot`, `harness.list`, `harness.test` | `allow` |
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
2. Run snapshot and event replay.
3. `run.start` and terminal run events.
4. Remote approvals and cancellation.
5. Pause/resume, bus messages and artifact references.

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
