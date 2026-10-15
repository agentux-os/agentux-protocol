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

console.log(`validated ${fixtures.valid.length} valid and ${fixtures.invalid.length} invalid AUXP fixtures`);
