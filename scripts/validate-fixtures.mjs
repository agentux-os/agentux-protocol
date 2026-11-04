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
const replayFixtures = JSON.parse(await readFile(resolve(root, "fixtures/v1/replay.json"), "utf8"));
const snapshotSchema = JSON.parse(await readFile(resolve(root, "schemas/v1/node-snapshot.schema.json"), "utf8"));
const snapshotFixtures = JSON.parse(await readFile(resolve(root, "fixtures/v1/node-snapshot.json"), "utf8"));

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
  if (message.method === "node.events.ack" && (!Number.isInteger(message.params.throughSequence) || message.params.throughSequence < 0 || !message.params.commandId)) return false;
  if (message.method === "node.snapshot" && (!message.params.commandId || !message.params.checkpointId || !Number.isInteger(message.params.requestedAfterSequence) || message.params.requestedAfterSequence < 0 || !Number.isInteger(message.params.pageSize) || message.params.pageSize < 1 || message.params.pageSize > 100 || (message.params.pageToken !== undefined && (typeof message.params.pageToken !== "string" || message.params.pageToken.length > 2048)))) return false;
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
      result_returned: ["fromAgentId"], task_completed: [], task_failed: [], artifact_available: ["artifacts"],
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
  if (first.schemaVersion !== "v1" || !first.checkpointId || !first.snapshotId || !first.organizationId || !first.nodeId || !Number.isInteger(first.highWaterSequence) || !Number.isInteger(first.requestedAfterSequence) || first.highWaterSequence < first.requestedAfterSequence || first.pageCount !== pages.length || first.pageCount > 10000 || first.entryCount < 0 || first.entryCount > 10000) return false;
  const ids = [];
  for (let i = 0; i < pages.length; i++) {
    const page = pages[i];
    if (page.schemaVersion !== first.schemaVersion || page.checkpointId !== first.checkpointId || page.snapshotId !== first.snapshotId || page.organizationId !== first.organizationId || page.nodeId !== first.nodeId || page.requestedAfterSequence !== first.requestedAfterSequence || page.highWaterSequence !== first.highWaterSequence || page.pageCount !== first.pageCount || page.entryCount !== first.entryCount || page.pageIndex !== i || !Array.isArray(page.entries) || page.entries.length > 100 || Buffer.byteLength(JSON.stringify(page), "utf8") > 1048576 || "events" in page || "activities" in page) return false;
    if ((i < pages.length - 1) !== (typeof page.nextPageToken === "string" && page.nextPageToken.length > 0)) return false;
    for (const entry of page.entries) {
      if (!entry.entryId || !["run", "removed"].includes(entry.kind) || (entry.kind === "run" && (entry.run?.runId !== entry.entryId || !entry.run.missionId || !entry.run.status || !entry.run.currentState || typeof entry.run.currentState !== "object")) || (entry.kind === "removed" && (!entry.missionId || !entry.removedAt))) return false;
      ids.push(entry.entryId);
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
const failureCodes = new Set(snapshotSchema.$defs?.failure?.properties?.code?.enum ?? []);
for (const failure of snapshotFixtures.failures.valid) {
  if (!failureCodes.has(failure.code) || !failure.checkpointId || typeof failure.retryable !== "boolean") throw new Error(`invalid snapshot failure fixture: ${failure.code}`);
}
for (const failure of snapshotFixtures.failures.invalid) {
  if (!failure.expectedError || (failureCodes.has(failure.code) && failure.checkpointId && typeof failure.retryable === "boolean")) throw new Error(`invalid snapshot failure accepted: ${failure.expectedError}`);
}
const limits = snapshotFixtures.limits;
if (limits.maxEntriesPerPage !== 100 || limits.maxPageBytes !== 1048576 || limits.maxEntries !== 10000 || limits.maxSnapshotBytes !== 67108864 || limits.snapshotExpirySeconds !== 900 || snapshotSchema.$defs.page.properties.entries.maxItems !== limits.maxEntriesPerPage || snapshotSchema.$defs.page.properties.entryCount.maximum !== limits.maxEntries) {
  throw new Error("snapshot fixtures and schema must pin page, total-size and expiry limits");
}
const negotiation = snapshotFixtures.negotiation;
if (!negotiation.nodeOffer.includes("node.snapshot_checkpoint.v1") || !negotiation.commandAccepted.includes("node.snapshot_checkpoint.v1") || negotiation.snapshotEnabled !== true || negotiation.legacySnapshotEnabled !== false || negotiation.legacyNodeOffer.includes("node.snapshot_checkpoint.v1")) {
  throw new Error("node snapshots must remain disabled unless the optional capability is mutually negotiated");
}
const eventBoundaryValid = (boundary) => boundary.stateReflectsThroughSequence >= boundary.highWaterSequence && boundary.firstReplaySequence === boundary.highWaterSequence + 1 && boundary.eventBeforeCapture <= boundary.highWaterSequence;
if (!eventBoundaryValid(snapshotFixtures.concurrentEvents.valid)) throw new Error("snapshot must cover all changes through its high-water sequence and replay the next sequence");
for (const boundary of snapshotFixtures.concurrentEvents.invalid) {
  if (!boundary.expectedError || eventBoundaryValid(boundary)) throw new Error(`invalid snapshot/event boundary accepted: ${boundary.expectedError}`);
}
if (snapshotSchema.$defs?.page?.properties?.schemaVersion?.const !== "v1" || snapshotSchema.$defs?.page?.properties?.entries?.maxItems !== 100 || snapshotSchema.$defs?.page?.required?.includes("highWaterSequence") !== true) {
  throw new Error("node snapshot schema must pin its version, page bound and checkpoint cursor");
}

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
