# AgentUX Protocol

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

Events are at-least-once. Consumers deduplicate by `eventId`, order by `(nodeId, sequence)`, and persist the last acknowledged sequence.

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

