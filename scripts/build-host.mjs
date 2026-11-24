import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { bundleCompiler } from "./host-compiler.mjs";
const out = "dist/conformance";
await bundleCompiler(out);
execFileSync("wasm-tools", [
  "component",
  "embed",
  "wit",
  "--dummy",
  "-o",
  "generated/contract.core.wasm",
]);
execFileSync("wasm-tools", [
  "component",
  "new",
  "generated/contract.core.wasm",
  "-o",
  "generated/contract.wasm",
]);
const wit = (f) =>
  execFileSync("wasm-tools", ["component", "wit", f], { encoding: "utf8" });
const meta = JSON.parse(readFileSync("generated/metadata.json"));
const contract = wit("generated/contract.wasm");
for (const filename of [meta.filename, "first-legal.wasm"]) {
  if (wit("generated/" + filename) !== contract)
    throw Error(filename + " has a different expanded WIT contract");
}
