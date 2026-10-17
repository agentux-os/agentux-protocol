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

if (schema.$defs?.baseParams?.properties?.protocolVersion?.const !== "v1") {
  throw new Error("AUXP schema must pin protocolVersion to v1");
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
  if ("apiKey" in message.params || "providerToken" in message.params) return false;
  return [...baseRequired, ...methodRequired(message.method)].every((field) => field in message.params);
};

for (const message of fixtures.valid) {
  if (!accepted(message)) throw new Error(`valid fixture was rejected: ${message.method}`);
}

for (const message of fixtures.invalid) {
  if (!message.expectedError) throw new Error("invalid fixture must document expectedError");
  if (accepted(message)) throw new Error(`invalid fixture was accepted: ${message.expectedError}`);
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

console.log(`validated ${fixtures.valid.length} valid and ${fixtures.invalid.length} invalid AUXP fixtures`);
