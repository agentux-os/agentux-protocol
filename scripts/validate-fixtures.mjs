import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const schema = JSON.parse(await readFile(resolve(root, "schemas/v1/messages.schema.json"), "utf8"));
const fixtures = JSON.parse(await readFile(resolve(root, "fixtures/v1/messages.json"), "utf8"));
const officeSchema = JSON.parse(await readFile(resolve(root, "schemas/v1/office-projection.schema.json"), "utf8"));
const officeFixtures = JSON.parse(await readFile(resolve(root, "fixtures/v1/office-projection.json"), "utf8"));
const approvalSchema = JSON.parse(await readFile(resolve(root, "schemas/v1/approval-resolution.schema.json"), "utf8"));
const approvalFixtures = JSON.parse(await readFile(resolve(root, "fixtures/v1/approval-responses.json"), "utf8"));
const activitySchema = JSON.parse(await readFile(resolve(root, "schemas/v1/execution-activity.schema.json"), "utf8"));
const artifactSchema = JSON.parse(await readFile(resolve(root, "schemas/v1/artifact-lifecycle.schema.json"), "utf8"));
const replayFixtures = JSON.parse(await readFile(resolve(root, "fixtures/v1/replay.json"), "utf8"));
const snapshotSchema = JSON.parse(await readFile(resolve(root, "schemas/v1/node-snapshot.schema.json"), "utf8"));
const snapshotFixtures = JSON.parse(await readFile(resolve(root, "fixtures/v1/node-snapshot.json"), "utf8"));
const requiredSnapshotProjections = ["approval", "artifact", "command", "gate", "harness", "limit", "node", "node_alert", "run", "session", "task"];

if (schema.$defs?.baseParams?.properties?.protocolVersion?.const !== "v1") {
  throw new Error("AUXP schema must pin protocolVersion to v1");
}
if (!schema.$defs?.agentIsolation?.required?.includes("mode") ||
    !schema.oneOf?.some((branch) => branch.allOf?.[1]?.properties?.method?.enum?.includes("run.updated") &&
      branch.allOf?.[1]?.properties?.params?.properties?.agentIsolation?.$ref === "#/$defs/agentIsolation")) {
  throw new Error("run event schema must define the typed agentIsolation field");
}

const methods = new Set(schema.oneOf.flatMap((branch) => {
  const method = branch.allOf[1].properties.method;
  return method.const ? [method.const] : method.enum;
}));
const baseRequired = schema.$defs.baseParams.required;
const methodRequired = (method) => schema.oneOf.find((branch) => {
  const definition = branch.allOf[1].properties.method;
  return definition.const === method || definition.enum?.includes(method);
})?.allOf[1].properties.params.required ?? [];
const accepted = (message) => {
  if (message.jsonrpc !== "2.0" || !methods.has(message.method) || !message.params) return false;
  if ("agentIsolation" in message.params) {
    const isolation = message.params.agentIsolation;
    if (!isolation || typeof isolation !== "object" || Array.isArray(isolation)) return false;
    if (!["run.updated", "run.completed", "run.failed"].includes(message.method)) return false;
    if (!schema.$defs.agentIsolation.properties.mode.enum.includes(isolation.mode) ||
        !schema.$defs.agentIsolation.properties.source.enum.includes(isolation.source)) return false;
    if (isolation.mode === "bubblewrap" && !schema.$defs.agentIsolation.properties.network.enum.includes(isolation.network)) return false;
    if (isolation.mode === "none" && "network" in isolation) return false;
    if (Object.keys(isolation).some((key) => !["mode", "network", "source"].includes(key))) return false;
  }
  if (message.method === "node.resume" && (!Number.isInteger(message.params.lastAcknowledgedSequence) || message.params.lastAcknowledgedSequence < 0 || !message.params.commandId)) return false;
  if (message.method === "node.events.ack" && (!Number.isInteger(message.params.throughSequence) || message.params.throughSequence < 0 || !message.params.commandId || ![undefined, "events", "snapshot_checkpoint"].includes(message.params.acknowledgementType) || (message.params.acknowledgementType === "snapshot_checkpoint" && (typeof message.params.checkpointId !== "string" || message.params.checkpointId.length < 1 || message.params.checkpointId.length > 128)) || (message.params.checkpointId !== undefined && message.params.acknowledgementType !== "snapshot_checkpoint"))) return false;
  if (message.method === "node.snapshot" && (!message.params.commandId || !message.params.checkpointId || !Number.isInteger(message.params.requestedAfterSequence) || message.params.requestedAfterSequence < 0 || !Number.isInteger(message.params.pageSize) || message.params.pageSize < 1 || message.params.pageSize > 100 || (message.params.pageToken !== undefined && (typeof message.params.pageToken !== "string" || message.params.pageToken.length > 2048)))) return false;
  if (message.method === "artifact.get" && (!message.params.commandId || !message.params.artifactId || [...message.params.artifactId].length > 128 || !/^[a-f0-9]{64}$/.test(message.params.sha256 ?? "") || !Number.isInteger(message.params.offset) || message.params.offset < 0 || message.params.offset > 26214400 || !Number.isInteger(message.params.maxBytes) || message.params.maxBytes < 1 || message.params.maxBytes > 262144)) return false;
  if (message.method === "node.heartbeat" && ("eventId" in message.params || "sequence" in message.params || !["ready", "draining", "error"].includes(message.params.status) || !message.params.capacity || typeof message.params.capacity !== "object" || Array.isArray(message.params.capacity))) return false;
  if (message.method === "node.status_changed") {
    const node = message.params.node;
    if (!node || typeof node !== "object" || Array.isArray(node) || node.nodeId !== message.params.nodeId || !["ready", "draining", "error"].includes(node.status) || !node.capacity || typeof node.capacity !== "object" || Array.isArray(node.capacity) || Object.keys(node).some((key) => !["nodeId", "status", "capacity"].includes(key))) return false;
  }
  if (message.method === "node.alert") {
    const event = message.params.alert;
    if (!event || typeof event !== "object" || Array.isArray(event)) return false;
    if (event.kind === "removed") {
      if (Object.keys(event).some((key) => !["kind", "alertId"].includes(key)) || typeof event.alertId !== "string" || !event.alertId.trim()) return false;
    } else if (event.kind === "state") {
      const alert = event.alert;
      const keys = ["alertId", "producer", "code", "conditionKey", "summary", "firstObservedAt", "lastObservedAt", "status"];
      const charCount = (value) => [...value].length;
      const timestamp = (value) => typeof value === "string" && /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/.test(value) && Number.isFinite(Date.parse(value));
      if (!alert || typeof alert !== "object" || Array.isArray(alert) || keys.some((key) => !(key in alert)) || Object.keys(alert).some((key) => !keys.includes(key)) || keys.slice(0, 4).some((key) => typeof alert[key] !== "string" || !alert[key].trim()) || typeof alert.summary !== "string" || !alert.summary.trim() || charCount(alert.summary) > 2000 || !["active", "resolved"].includes(alert.status) || !timestamp(alert.firstObservedAt) || !timestamp(alert.lastObservedAt) || Date.parse(alert.lastObservedAt) < Date.parse(alert.firstObservedAt)) return false;
      if (Object.keys(event).some((key) => !["kind", "alert"].includes(key))) return false;
    } else return false;
  }
  if ("apiKey" in message.params || "providerToken" in message.params) return false;
  const forbidden = (value) => {
    if (Array.isArray(value)) return value.some(forbidden);
    if (!value || typeof value !== "object") return false;
    return Object.entries(value).some(([key, child]) =>
      ["apiKey", "providerToken", "credentials", "chainOfThought", "chain_of_thought", "privateChainOfThought", "reasoning", "privateReasoning", "toolInput", "rawToolInput", "rawInput", "toolOutput"].includes(key) || forbidden(child));
  };
  if (message.method === "execution.activity" && forbidden(message.params.activity)) return false;
  if (message.method === "execution.activity") {
    const activity = message.params.activity;
    const charCount = (value) => [...value].length;
    if (!activity || activity.schemaVersion !== "v1" || !activity.activityId || !activity.missionId || !activity.runId || !activity.agentId || !activity.roleId || typeof activity.summary !== "string" || charCount(activity.summary) > 2000) return false;
    for (const key of ["activityId", "missionId", "runId", "agentId", "roleId", "sessionId", "taskId", "parentTaskId", "causationId", "delegationId", "toolCallId"]) {
      if (activity[key] !== undefined && (!activity[key] || charCount(activity[key]) > 128)) return false;
    }
    if (!activitySchema.$defs.activity.properties.type.enum.includes(activity.type)) return false;
    if (activity.type === "task_created" && (typeof activity.taskName !== "string" || !activity.taskName || charCount(activity.taskName) > 256)) return false;
    if (["task_created", "task_status_changed"].includes(activity.type) && !activity.status) return false;
    if (["task_created", "task_status_changed"].includes(activity.type) && !["pending", "running", "blocked", "completed", "failed", "cancelled"].includes(activity.status)) return false;
    if (["task_created", "task_status_changed", "task_completed", "task_failed"].includes(activity.type) && !activity.taskId) return false;
    if (activity.type.startsWith("tool_") && (!activity.toolName || !activity.toolCallId)) return false;
    if (["delegated", "handoff", "task_assigned"].includes(activity.type) && (!activity.delegationId || !activity.toAgentId || !activity.toRoleId)) return false;
    if (activity.type === "result_returned" && (!activity.delegationId || !activity.fromAgentId)) return false;
    if (activity.attempt !== undefined && (!Number.isInteger(activity.attempt) || activity.attempt < 1)) return false;
    if (activity.toolName && charCount(activity.toolName) > 128) return false;
    if (activity.toAgentId && charCount(activity.toAgentId) > 128) return false;
    if (activity.toRoleId && charCount(activity.toRoleId) > 128) return false;
    if (activity.fromAgentId && charCount(activity.fromAgentId) > 128) return false;
    if (activity.type === "tool_progress" && (!Number.isInteger(activity.progress) || activity.progress < 0 || activity.progress > 100)) return false;
    if (activity.type === "artifact_available" && (!Array.isArray(activity.artifacts) || activity.artifacts.length > 16 || activity.artifacts.some((ref) => !ref.artifactId || charCount(ref.artifactId) > 128 || !ref.mediaType || charCount(ref.mediaType) > 127 || !Number.isInteger(ref.byteLength) || ref.byteLength < 0 || ref.byteLength > 26214400 || !/^[A-Fa-f0-9]{64}$/.test(ref.sha256 ?? "") || charCount(ref.uri ?? "") > 2048 || !/^(https:\/\/|artifact:\/\/)/.test(ref.uri ?? "")))) return false;
    const variantFields = {
      plan_created: [], task_created: ["taskName", "status"], task_status_changed: ["status"],
      tool_started: ["toolName"], tool_progress: ["toolName", "progress"], tool_completed: ["toolName"], tool_failed: ["toolName"],
      task_assigned: ["toAgentId", "toRoleId"], delegated: ["toAgentId", "toRoleId"], handoff: ["toAgentId", "toRoleId"],
      result_returned: ["fromAgentId"], task_completed: [], task_failed: [], artifact_available: ["artifacts"], artifact_removed: ["artifactId"],
    };
    const commonFields = new Set(["schemaVersion", "activityId", "missionId", "runId", "agentId", "roleId", "sessionId", "taskId", "parentTaskId", "attempt", "causationId", "delegationId", "toolCallId", "type", "summary"]);
    if (Object.keys(activity).some((key) => !commonFields.has(key) && !variantFields[activity.type].includes(key))) return false;
  }
  return [...baseRequired, ...methodRequired(message.method)].every((field) => field in message.params);
};

if (activitySchema.$defs?.activity?.properties?.schemaVersion?.const !== "v1" ||
    activitySchema.$defs?.activity?.properties?.summary?.maxLength !== 2000 ||
    activitySchema.$defs?.activity?.properties?.artifacts?.maxItems !== 16) {
  throw new Error("typed activity schema must pin version and payload bounds");
}

if (!schema.oneOf.some((branch) => branch.allOf?.[1]?.properties?.method?.const === "artifact.get") ||
    artifactSchema.$defs?.getParams?.properties?.maxBytes?.maximum !== 262144 ||
    artifactSchema.$defs?.chunkResult?.properties?.byteLength?.maximum !== 26214400) {
  throw new Error("artifact lifecycle schema must pin request and content bounds");
}

for (const message of fixtures.valid) {
  if (!accepted(message)) throw new Error(`valid fixture was rejected: ${message.method}`);
}

for (const message of fixtures.invalid) {
  if (!message.expectedError) throw new Error("invalid fixture must document expectedError");
  if (accepted(message)) throw new Error(`invalid fixture was accepted: ${message.expectedError}`);
}

for (const request of snapshotFixtures.requests.valid) {
  if (!accepted(request)) throw new Error(`valid snapshot request was rejected: ${request.id}`);
}
for (const request of snapshotFixtures.requests.invalid) {
  if (!request.expectedError || accepted(request)) throw new Error(`invalid snapshot request was accepted: ${request.expectedError}`);
}

const validSnapshotPages = (pages) => {
  if (!Array.isArray(pages) || pages.length === 0) return false;
  const first = pages[0];
  if (first.schemaVersion !== "v1" || !first.checkpointId || !first.snapshotId || !first.organizationId || !first.nodeId || !Number.isInteger(first.highWaterSequence) || !Number.isInteger(first.requestedAfterSequence) || first.highWaterSequence < first.requestedAfterSequence || first.pageCount !== pages.length || first.pageCount > 10000 || first.entryCount < 0 || first.entryCount > 10000 || first.projectionCoverage?.join(",") !== requiredSnapshotProjections.join(",")) return false;
  const ids = [];
  for (let i = 0; i < pages.length; i++) {
    const page = pages[i];
    if (page.schemaVersion !== first.schemaVersion || page.checkpointId !== first.checkpointId || page.snapshotId !== first.snapshotId || page.organizationId !== first.organizationId || page.nodeId !== first.nodeId || page.requestedAfterSequence !== first.requestedAfterSequence || page.highWaterSequence !== first.highWaterSequence || page.pageCount !== first.pageCount || page.entryCount !== first.entryCount || page.projectionCoverage?.join(",") !== requiredSnapshotProjections.join(",") || page.pageIndex !== i || page.pageIndex >= page.pageCount || !Array.isArray(page.entries) || page.entries.length > 100 || Buffer.byteLength(JSON.stringify(page), "utf8") > 1048576 || "events" in page || "activities" in page) return false;
    if (i === 0 ? page.pageToken !== undefined : page.pageToken !== pages[i - 1].nextPageToken) return false;
    if ((i < pages.length - 1) !== (typeof page.nextPageToken === "string" && page.nextPageToken.length > 0)) return false;
    for (const entry of page.entries) {
      if (!entry.entryId || !requiredSnapshotProjections.includes(entry.projectionType) || !entry.entryId.startsWith(`${entry.projectionType}:`) || entry.entryId.length === entry.projectionType.length + 1) return false;
      if (entry.kind === "state" && (!entry.state || typeof entry.state !== "object" || Array.isArray(entry.state) || !entry.state.currentState || typeof entry.state.currentState !== "object" || Array.isArray(entry.state.currentState))) return false;
      if (entry.kind === "state" && entry.projectionType === "node") {
        const node = entry.state;
        const current = node.currentState;
        if (!node.nodeId || !["ready", "draining", "error"].includes(node.status) || !node.capacity || typeof node.capacity !== "object" || Array.isArray(node.capacity) || current.nodeId !== node.nodeId || !["ready", "draining", "error"].includes(current.status) || !current.capacity || typeof current.capacity !== "object" || Array.isArray(current.capacity)) return false;
      }
      if (entry.kind === "state" && entry.projectionType === "node_alert") {
        const alert = entry.state;
        const current = alert.currentState;
        const keys = ["alertId", "producer", "code", "conditionKey", "summary", "firstObservedAt", "lastObservedAt", "status"];
        const timestamp = (value) => typeof value === "string" && /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/.test(value) && Number.isFinite(Date.parse(value));
        if (alert.alertId !== current.alertId || keys.some((key) => !(key in alert) || !(key in current)) || [alert, current].some((item) => keys.slice(0, 4).some((key) => typeof item[key] !== "string" || !item[key].trim()) || !["active", "resolved"].includes(item.status) || typeof item.summary !== "string" || !item.summary.trim() || [...item.summary].length > 2000 || !timestamp(item.firstObservedAt) || !timestamp(item.lastObservedAt) || Date.parse(item.lastObservedAt) < Date.parse(item.firstObservedAt))) return false;
      }
      if (entry.kind === "state" && entry.projectionType === "run" && (entry.state.owner !== "command" || !entry.state.commandId || !entry.state.organizationId || entry.state.organizationId !== page.organizationId || entry.state.runId !== entry.entryId.slice("run:".length) || entry.state.currentState.sessions === undefined || entry.state.currentState.gates === undefined || entry.state.currentState.tasks === undefined)) return false;
      if (entry.kind === "removed" && (!Number.isInteger(entry.removedAtSequence) || entry.removedAtSequence <= first.requestedAfterSequence || entry.removedAtSequence > first.highWaterSequence || (entry.projectionType === "run" && (entry.runId !== entry.entryId.slice("run:".length) || !entry.runId)))) return false;
      if (!(["state", "removed"].includes(entry.kind))) return false;
      ids.push(`${entry.projectionType}:${entry.entryId}`);
    }
  }
  return ids.length === first.entryCount && ids.every((id, i) => i === 0 || ids[i - 1] < id);
};
for (const pages of snapshotFixtures.pages.valid) {
  if (!validSnapshotPages(pages)) throw new Error("valid snapshot pages violate immutable manifest, completeness, ordering or pagination invariants");
}
for (const fixture of snapshotFixtures.pages.invalid) {
  if (!fixture.expectedError || validSnapshotPages(fixture.pages)) throw new Error(`invalid snapshot pages accepted: ${fixture.expectedError}`);
}
const ackValid = (ack) => ack.checkpointId && ack.allPagesServed === true && ack.scopeMatches === true && ack.throughSequence === ack.highWaterSequence && ack.throughSequence >= ack.requestedAfterSequence;
if (!ackValid({...snapshotFixtures.checkpointAck.valid, highWaterSequence:57})) throw new Error("valid checkpoint ACK violates checkpoint binding");
for (const ack of snapshotFixtures.checkpointAck.invalid) {
  if (!ack.expectedError || ackValid(ack)) throw new Error(`invalid checkpoint ACK accepted: ${ack.expectedError}`);
}
const snapshotAckMethod = schema.oneOf.find((branch) => branch.allOf[1].properties.method.const === "node.events.ack");
const checkpointAckRule = snapshotAckMethod?.allOf[1].properties.params.allOf?.find((rule) => rule.if?.properties?.acknowledgementType?.const === "snapshot_checkpoint");
if (!snapshotAckMethod || !checkpointAckRule?.then?.required?.includes("checkpointId") || !schema.$defs.baseParams.properties.checkpointId) throw new Error("checkpoint ACK must bind node.events.ack to a required checkpointId");
for (const request of snapshotFixtures.checkpointAck.requests.valid) {
  if (!accepted(request)) throw new Error("valid checkpoint ACK request refused");
}
for (const request of snapshotFixtures.checkpointAck.requests.invalid) {
  if (!request.expectedError || accepted(request)) throw new Error(`invalid checkpoint ACK accepted: ${request.expectedError}`);
}
const failureCodes = new Set(snapshotSchema.$defs?.failure?.properties?.code?.enum ?? []);
const retryableByCode = {snapshot_unavailable:true, snapshot_expired:true, snapshot_too_large:false, snapshot_entry_too_large:false, checkpoint_conflict:false, stale_checkpoint:false, checkpoint_coverage_incomplete:false, checkpoint_incomplete:true, checkpoint_scope_mismatch:false, checkpoint_sequence_mismatch:false};
for (const failure of snapshotFixtures.failures.valid) {
  if (!failureCodes.has(failure.code) || !failure.checkpointId || failure.retryable !== retryableByCode[failure.code]) throw new Error(`invalid snapshot failure fixture: ${failure.code}`);
}
for (const failure of snapshotFixtures.failures.invalid) {
  if (!failure.expectedError || (failureCodes.has(failure.code) && failure.checkpointId && failure.retryable === retryableByCode[failure.code])) throw new Error(`invalid snapshot failure accepted: ${failure.expectedError}`);
}
const limits = snapshotFixtures.limits;
if (limits.maxEntriesPerPage !== 100 || limits.maxPageBytes !== 1048576 || limits.maxEntries !== 10000 || limits.maxSnapshotBytes !== 67108864 || limits.snapshotExpirySeconds !== 900 || snapshotSchema.$defs.page.properties.entries.maxItems !== limits.maxEntriesPerPage || snapshotSchema.$defs.page.properties.entryCount.maximum !== limits.maxEntries) {
  throw new Error("snapshot fixtures and schema must pin page, total-size and expiry limits");
}
const negotiation = snapshotFixtures.negotiation;
if (!negotiation.nodeOffer.includes("node.snapshot_checkpoint.v1") || !negotiation.nodeOffer.includes("node.event_replay.v1") || !negotiation.commandAccepted.includes("node.snapshot_checkpoint.v1") || !negotiation.commandAccepted.includes("node.event_replay.v1") || negotiation.snapshotEnabled !== true || negotiation.legacySnapshotEnabled !== false || negotiation.legacyNodeOffer.includes("node.snapshot_checkpoint.v1") || negotiation.noReplayCapability.snapshotEnabled !== false || negotiation.noReplayCapability.nodeOffer.includes("node.event_replay.v1")) {
  throw new Error("node snapshots must remain disabled unless the optional capability is mutually negotiated");
}
const eventBoundaryValid = (boundary) => boundary.stateReflectsThroughSequence >= boundary.highWaterSequence && boundary.firstReplaySequence === boundary.highWaterSequence + 1 && boundary.eventBeforeCapture <= boundary.highWaterSequence;
if (!eventBoundaryValid(snapshotFixtures.concurrentEvents.valid)) throw new Error("snapshot must cover all changes through its high-water sequence and replay the next sequence");
for (const boundary of snapshotFixtures.concurrentEvents.invalid) {
  if (!boundary.expectedError || eventBoundaryValid(boundary)) throw new Error(`invalid snapshot/event boundary accepted: ${boundary.expectedError}`);
}
const staleCheckpoint = snapshotFixtures.staleCheckpoint;
if (staleCheckpoint.valid.requestedAfterSequence !== staleCheckpoint.valid.lastAcceptedAck || staleCheckpoint.valid.beginAccepted !== true || staleCheckpoint.invalid.some((item) => !item.expectedError || item.beginAccepted !== false && item.ackAccepted !== false || (item.requestedAfterSequence !== undefined && item.requestedAfterSequence === item.lastAcceptedAck) || (item.lastAcceptedAckAtStart !== undefined && item.lastAcceptedAckAtStart === item.lastAcceptedAckAtAck))) throw new Error("snapshot capture and checkpoint ACK must compare against the node's last accepted ACK");
const scopeFixture = snapshotFixtures.scope;
const snapshotRunIds = snapshotFixtures.pages.valid.flatMap((pages) => pages.flatMap((page) => page.entries.filter((entry) => entry.kind === "state" && entry.projectionType === "run").map((entry) => entry.state.runId)));
if (scopeFixture.includedRun.included !== true || scopeFixture.includedRun.organizationId !== scopeFixture.authenticatedOrganizationId || scopeFixture.includedRun.owner !== "command" || !scopeFixture.includedRun.commandId || scopeFixture.excludedLocalOnlyRun.included !== false || snapshotRunIds.includes(scopeFixture.excludedLocalOnlyRun.runId)) throw new Error("node-local runs without authenticated Command ownership must be excluded from snapshot inventory");
const tombstone = snapshotFixtures.tombstones.valid;
if (!(tombstone.requestedAfterSequence < tombstone.removedAtSequence && tombstone.removedAtSequence <= tombstone.highWaterSequence && tombstone.mustInclude === true && tombstone.minimumRetentionSeconds === 900 && tombstone.pinnedSnapshotRetainsCopy === true)) throw new Error("removal tombstones must survive minimum retention and pinned snapshots");
if (snapshotFixtures.tombstones.compaction.fullInventoryAuthoritative !== true || snapshotFixtures.tombstones.compaction.omissionRemovesPreviouslyProjectedKey !== true || snapshotFixtures.tombstones.compaction.minimumRetentionSeconds !== 900 || snapshotFixtures.tombstones.compaction.mayCompactWhenNoPinnedSnapshot !== true) throw new Error("tombstone compaction must preserve full-inventory reconciliation semantics");
for (const item of snapshotFixtures.tombstones.invalid) {
  const valid = item.compactedAfterSeconds === undefined || item.compactedAfterSeconds >= item.minimumRetentionSeconds;
  const validInclusion = item.included !== true || item.removedAtSequence > item.requestedAfterSequence;
  const validPinnedCopy = item.compactedCopyChanged !== true || item.pinnedSnapshot !== true;
  if (!item.expectedError || (valid && validInclusion && validPinnedCopy)) throw new Error(`invalid tombstone compaction accepted: ${item.expectedError}`);
}
if (snapshotSchema.$defs?.page?.properties?.schemaVersion?.const !== "v1" || snapshotSchema.$defs?.page?.properties?.entries?.maxItems !== 100 || snapshotSchema.$defs?.page?.required?.includes("highWaterSequence") !== true) {
  throw new Error("node snapshot schema must pin its version, page bound and checkpoint cursor");
}
if (schema.$defs?.nodeSnapshotResult?.$ref !== "node-snapshot.schema.json#/$defs/page") throw new Error("node.snapshot result must use the shared snapshot page schema");

for (const result of replayFixtures.valid) {
  const expired = result.status === "cursor_expired";
  if (!result.nodeId || result.highWaterSequence < result.requestedAfterSequence ||
      (expired && (!result.snapshotRequired || result.eventsReplayed !== 0)) ||
      (!expired && (result.snapshotRequired || result.requestedAfterSequence + 1 < result.oldestAvailableSequence || result.eventsReplayed !== result.highWaterSequence - result.requestedAfterSequence))) {
    throw new Error("valid replay fixture violates cursor invariants");
  }
}
for (const result of replayFixtures.invalid) {
  const expired = result.status === "cursor_expired";
  const accepted = Boolean(result.snapshotRequired === expired && result.nodeId && result.highWaterSequence >= result.requestedAfterSequence &&
    (expired ? result.eventsReplayed === 0 && result.requestedAfterSequence + 1 < result.oldestAvailableSequence :
      result.requestedAfterSequence + 1 >= result.oldestAvailableSequence && result.eventsReplayed === result.highWaterSequence - result.requestedAfterSequence));
  if (accepted) throw new Error(`invalid replay fixture accepted: ${result.expectedError}`);
}

const activityTypes = new Set(fixtures.valid.filter((message) => message.method === "execution.activity").map((message) => message.params.activity.type));
if (activityTypes.size !== activitySchema.$defs.activity.properties.type.enum.length ||
    activitySchema.$defs.activity.properties.type.enum.some((type) => !activityTypes.has(type))) {
  throw new Error("valid message fixtures must cover every typed activity variant");
}
const resumeRequired = ["status", "nodeId", "requestedAfterSequence", "oldestAvailableSequence", "highWaterSequence", "eventsReplayed", "snapshotRequired"];
if (resumeRequired.some((key) => !schema.$defs.resumeResult.required.includes(key)) ||
    !schema.oneOf.some((branch) => branch.allOf[1].properties.method.const === "node.events.ack")) {
  throw new Error("replay response and acknowledgement contracts are incomplete");
}

if (officeSchema.$defs?.projectionMetadata?.required?.join(",") !== "schemaVersion,definitionVersion,projectedAt,source,freshness") {
  throw new Error("Office projection schema must require distributed freshness metadata");
}
const projection = officeFixtures.valid;
if (!projection.organizationId || !projection.lastEventId || !Array.isArray(projection.events)) {
  throw new Error("Office projection fixture must carry organization and replay identity");
}
if (!officeFixtures.invalid.some((item) => item.expectedError === "missing lastEventId")) {
  throw new Error("Office fixtures must cover missing replay identity");
}
if (approvalSchema.$defs?.conflict?.properties?.code?.enum?.join(",") !== "denied,expired,already_decided,not_found,stale_version") {
  throw new Error("Approval schema must pin typed decision conflicts");
}
if (!approvalFixtures.valid.some((item) => item.idempotentReplay === true)) {
  throw new Error("Approval fixtures must cover idempotent replay");
}
if (approvalSchema.$defs?.request?.required?.join(",") !== "organizationId,decision,idempotencyKey") {
  throw new Error("Approval request must require organization-scoped idempotency identity");
}
if (approvalFixtures.valid.some((item) => item.ok === true && !item.organizationId)) {
  throw new Error("Successful approval responses must identify their organization");
}
if (!approvalFixtures.invalid.some((item) => item.expectedError === "missing organizationId")) {
  throw new Error("Approval fixtures must reject successful responses without organizationId");
}
if (approvalFixtures.requests.some((request) => !request.organizationId || !request.idempotencyKey)) {
  throw new Error("Approval request fixtures must include organization and idempotency identity");
}

console.log(`validated ${fixtures.valid.length} valid and ${fixtures.invalid.length} invalid AUXP fixtures; ${replayFixtures.valid.length} valid and ${replayFixtures.invalid.length} invalid replay fixtures`);
