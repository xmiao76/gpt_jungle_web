import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, truncate, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import test from "node:test";

import {
  DEFAULT_RULES_FINGERPRINT,
  ENGINE_ABI,
  MAX_ARTIFACT_BYTES_EXCLUSIVE,
  RUNTIME_MANIFEST_SCHEMA,
  buildEngineArtifactManifest,
  writeEngineArtifactManifest,
} from "./engine-artifact-manifest.mjs";

async function fixture() {
  const directory = await mkdtemp(join(tmpdir(), "jungle-engine-artifacts-"));
  await mkdir(join(directory, "nested"));
  await writeFile(join(directory, "opening.book"), "opening-data");
  await writeFile(join(directory, "nested", "evaluator.nnue"), "model-data");
  return directory;
}

test("manifest is deterministic and cache descriptors have the runtime shape", async (context) => {
  const directory = await fixture();
  context.after(() => rm(directory, { recursive: true, force: true }));
  const options = { inputDirectory: directory, urlPrefix: "/generated/engine" };
  const first = await buildEngineArtifactManifest(options);
  const second = await buildEngineArtifactManifest(options);
  assert.equal(first.json, second.json);
  assert.equal(first.sha256, second.sha256);
  assert.equal(first.manifest.schema, RUNTIME_MANIFEST_SCHEMA);
  assert.equal(first.manifest.version, 2);
  assert.equal(first.manifest.engineAbi, ENGINE_ABI);
  assert.equal(first.manifest.rulesFingerprint, DEFAULT_RULES_FINGERPRINT);
  assert.deepEqual(
    first.manifest.artifacts.map(({ kind }) => kind).sort(),
    ["book", "model"],
  );
  for (const descriptor of first.manifest.artifacts) {
    for (const field of ["id", "url", "sha256", "bytes", "kind", "rulesFingerprint"]) {
      assert.ok(Object.hasOwn(descriptor, field), `descriptor is missing ${field}`);
    }
    assert.match(descriptor.url, new RegExp(`\\?v=${descriptor.sha256.slice(0, 16)}$`));
  }
});

test("writing inside the artifact tree excludes the output deterministically", async (context) => {
  const directory = await fixture();
  context.after(() => rm(directory, { recursive: true, force: true }));
  const outputPath = join(directory, "runtime-manifest.json");
  const first = await writeEngineArtifactManifest({ inputDirectory: directory, outputPath });
  const firstBytes = await readFile(outputPath, "utf8");
  const second = await writeEngineArtifactManifest({ inputDirectory: directory, outputPath });
  assert.equal(second.json, first.json);
  assert.equal(await readFile(outputPath, "utf8"), firstBytes);
});

test("content changes change both artifact and manifest hashes", async (context) => {
  const directory = await fixture();
  context.after(() => rm(directory, { recursive: true, force: true }));
  const before = await buildEngineArtifactManifest({ inputDirectory: directory });
  await writeFile(join(directory, "opening.book"), "different-opening-data");
  const after = await buildEngineArtifactManifest({ inputDirectory: directory });
  assert.notEqual(before.sha256, after.sha256);
  assert.notEqual(
    before.manifest.artifacts.find(({ kind }) => kind === "book").sha256,
    after.manifest.artifacts.find(({ kind }) => kind === "book").sha256,
  );
});

test("build rejects a file at the exclusive 25 MiB ceiling", async (context) => {
  const directory = await mkdtemp(join(tmpdir(), "jungle-engine-oversize-"));
  context.after(() => rm(directory, { recursive: true, force: true }));
  const path = join(directory, "oversize.book");
  await writeFile(path, "x");
  await truncate(path, MAX_ARTIFACT_BYTES_EXCLUSIVE);
  await assert.rejects(
    buildEngineArtifactManifest({ inputDirectory: directory }),
    /must be smaller than/,
  );
});
