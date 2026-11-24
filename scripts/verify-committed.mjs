import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";
const sums = readFileSync("release/SHA256.txt", "utf8").trim().split(/\r?\n/);
assert.equal(sums.length, 2);
const readme = readFileSync("README.md", "utf8");
let component;
for (const [i, line] of sums.entries()) {
  const match = /^([0-9a-f]{64}) {2}([a-z0-9.-]+)$/.exec(line);
  assert(match, "Malformed checksum line");
  const bytes = readFileSync("release/" + match[2]);
  assert.equal(createHash("sha256").update(bytes).digest("hex"), match[1]);
  assert(readme.includes(match[1]), "README release checksum differs");
  if (i === 0) {
    assert(match[2].endsWith(".wasm"));
    component = bytes;
  } else assert.equal(match[2], "index.html");
}
assert(component);
assert(
  readFileSync("release/index.html", "utf8").includes(
    component.toString("base64"),
  ),
  "Embedded component differs from download",
);
console.log("Both checked-in SHA-256 values and the embedded component match.");
