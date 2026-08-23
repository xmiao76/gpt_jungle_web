import assert from "node:assert/strict";
import { copyFile, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import test from "node:test";

import { buildEngineWasmArtifacts, wasmPackArguments } from "./engine-build-wasm.mjs";

test("wasm-pack arguments put cargo features after the separator", () => {
  const arguments_ = wasmPackArguments({
    crateDirectory: "crate",
    outputDirectory: "output",
    features: ["simd", "threads"],
  });
  assert.deepEqual(arguments_.slice(-4), ["--", "--locked", "--features", "simd,threads"]);
});

test("orchestrator publishes both variants, mandatory tablebase, and runtime manifest", async (context) => {
  const repositoryRoot = await mkdtemp(join(tmpdir(), "jungle-engine-build-test-"));
  context.after(() => rm(repositoryRoot, { recursive: true, force: true }));
  const generatedRoot = join(repositoryRoot, "public", "generated");
  await mkdir(join(generatedRoot, "tablebase"), { recursive: true });
  await writeFile(join(generatedRoot, "tablebase", "stale.jgtbs"), "stale");
  await writeFile(join(generatedRoot, "python-runtime.json"), "legacy");
  await writeFile(join(generatedRoot, "python-runtime.0123456789abcdef.zip"), "legacy");
  const crateDirectory = join(repositoryRoot, "engine", "crates", "jungle-wasm");
  await mkdir(crateDirectory, { recursive: true });
  const strengthMetadataPath = join(repositoryRoot, "engine", "artifacts", "manifest-metadata.json");
  await mkdir(join(repositoryRoot, "engine", "artifacts"), { recursive: true });
  await writeFile(
    strengthMetadataPath,
    '{"schema":"gpt-jungle-strength-metadata","version":1,"artifacts":{},"materials":{}}',
  );
  const sourceAsset = resolve(
    import.meta.dirname,
    "..",
    "python",
    "jungle",
    "ai",
    "data",
    "two_piece_v1.jgtb",
  );
  const legacyAssetPath = join(repositoryRoot, "two_piece_v1.jgtb");
  await copyFile(sourceAsset, legacyAssetPath);

  const calls = [];
  const executeWasmPack = async (options) => {
    calls.push(options);
    await mkdir(options.outputDirectory, { recursive: true });
    await writeFile(join(options.outputDirectory, "jungle_wasm.js"), `export default ${JSON.stringify(options.features)};`);
    await writeFile(join(options.outputDirectory, "jungle_wasm_bg.wasm"), Buffer.from([0, 97, 115, 109]));
  };
  await buildEngineWasmArtifacts({
    repositoryRoot,
    generatedRoot,
    legacyAssetPath,
    strengthMetadataPath,
    executeWasmPack,
  });

  assert.equal(calls.length, 2);
  assert.deepEqual(calls[0].features, ["simd", "threads"]);
  assert.deepEqual(calls[1].features, []);
  for (const variant of ["scalar", "threaded"]) {
    await readFile(join(generatedRoot, "jungle-wasm", variant, "jungle_wasm.js"));
    await readFile(join(generatedRoot, "jungle-wasm", variant, "jungle_wasm_bg.wasm"));
  }
  await readFile(join(generatedRoot, "tablebase", "two_piece_v1.jgtb"));
  await assert.rejects(readFile(join(generatedRoot, "tablebase", "stale.jgtbs")));
  await assert.rejects(readFile(join(generatedRoot, "python-runtime.json")));
  await assert.rejects(readFile(join(generatedRoot, "python-runtime.0123456789abcdef.zip")));
  const manifest = JSON.parse(await readFile(join(generatedRoot, "engine-runtime-manifest.json"), "utf8"));
  assert.equal(manifest.schema, "gpt-jungle-engine-runtime");
  assert.equal(manifest.engineAbi, "jungle-wasm-v2");
  assert.deepEqual(manifest.variants.map(({ id }) => id), ["threaded", "scalar"]);
  assert.equal(manifest.artifacts.length, 5);
  assert.equal(manifest.artifacts.find(({ id }) => id === "tablebase:two-piece-v1").kind, "tablebase");
});
