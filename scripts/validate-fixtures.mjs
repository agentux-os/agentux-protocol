import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const schema = JSON.parse(await readFile(resolve(root, "schemas/v1/messages.schema.json"), "utf8"));
const fixtures = JSON.parse(await readFile(resolve(root, "fixtures/v1/messages.json"), "utf8"));

if (schema.$defs?.baseParams?.properties?.protocolVersion?.const !== "v1") {
  throw new Error("AUXP schema must pin protocolVersion to v1");
}

const methods = new Set(schema.oneOf.flatMap((branch) => {
  const method = branch.allOf[1].properties.method;
  return method.const ? [method.const] : method.enum;
}));
for (const message of fixtures.valid) {
  if (!methods.has(message.method)) throw new Error(`fixture method is not in schema: ${message.method}`);
  for (const field of ["jsonrpc", "method", "params"]) {
    if (!(field in message)) throw new Error(`fixture is missing ${field}`);
  }
  if (message.jsonrpc !== "2.0") throw new Error("fixture is not JSON-RPC 2.0");
  for (const field of ["protocolVersion", "organizationId", "nodeId", "messageId", "occurredAt"]) {
    if (!(field in message.params)) throw new Error(`fixture params is missing ${field}`);
  }
}

for (const message of fixtures.invalid) {
  if (!message.expectedError) throw new Error("invalid fixture must document expectedError");
}

console.log(`validated ${fixtures.valid.length} valid and ${fixtures.invalid.length} invalid AUXP fixtures`);
