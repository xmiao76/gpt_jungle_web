import { spawn } from "node:child_process";
import { copyFile, mkdir, mkdtemp, readFile, readdir, rename, rm } from "node:fs/promises";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

import {
  DEFAULT_RULES_FINGERPRINT,
  writeEngineArtifactManifest,
} from "./engine-artifact-manifest.mjs";

const SCRIPT_DIRECTORY = dirname(fileURLToPath(import.meta.url));
const DEFAULT_REPOSITORY_ROOT = resolve(SCRIPT_DIRECTORY, "..");

function ensureGeneratedTarget(target, generatedRoot) {
  const resolvedTarget = resolve(target);
  const resolvedRoot = resolve(generatedRoot);
  if (resolvedTarget === resolvedRoot || !resolvedTarget.startsWith(`${resolvedRoot}${sep}`)) {
    throw new Error(`Refusing to replace a directory outside the generated artifact tree: ${resolvedTarget}`);
  }
  return resolvedTarget;
}

async function replaceDirectory(source, target, generatedRoot) {
  const safeTarget = ensureGeneratedTarget(target, generatedRoot);
  await mkdir(dirname(safeTarget), { recursive: true });
  await rm(safeTarget, { recursive: true, force: true });
  await rename(source, safeTarget);
}

async function removeLegacyPythonArtifacts(generatedRoot) {
  const entries = await readdir(generatedRoot, { withFileTypes: true });
  for (const entry of entries) {
    if (
      entry.isFile() &&
      (entry.name === "python-runtime.json" || /^python-runtime\.[a-f0-9]{16}\.zip$/.test(entry.name))
    ) {
      await rm(join(generatedRoot, entry.name));
    }
  }
}

export function wasmPackArguments({ crateDirectory, outputDirectory, features = [] }) {
  const arguments_ = [
    "build",
    crateDirectory,
    "--target",
    "web",
    "--release",
    "--out-dir",
    outputDirectory,
    "--out-name",
    "jungle_wasm",
    "--no-pack",
    "--no-typescript",
  ];
  arguments_.push("--", "--locked");
  if (features.length !== 0) arguments_.push("--features", features.join(","));
  return arguments_;
}

export function runWasmPack({
  repositoryRoot,
  crateDirectory,
  outputDirectory,
  features = [],
  rustFlags = "",
}) {
  return new Promise((resolvePromise, rejectPromise) => {
    const inheritedFlags = process.env.RUSTFLAGS?.trim();
    const combinedFlags = [inheritedFlags, rustFlags].filter(Boolean).join(" ");
    const child = spawn(
      "wasm-pack",
      wasmPackArguments({ crateDirectory, outputDirectory, features }),
      {
        cwd: repositoryRoot,
        env: { ...process.env, ...(combinedFlags ? { RUSTFLAGS: combinedFlags } : {}) },
        stdio: "inherit",
        windowsHide: true,
      },
    );
    child.once("error", rejectPromise);
    child.once("exit", (code, signal) => {
      if (code === 0) resolvePromise();
      else rejectPromise(new Error(`wasm-pack failed (${signal ?? `exit ${code}`}).`));
    });
  });
}

export async function buildEngineWasmArtifacts({
  repositoryRoot = DEFAULT_REPOSITORY_ROOT,
  generatedRoot = join(repositoryRoot, "public", "generated"),
  legacyAssetPath = join(repositoryRoot, "python", "jungle", "ai", "data", "two_piece_v1.jgtb"),
  optionalTablebasePaths = [],
  strengthMetadataPath = join(repositoryRoot, "engine", "artifacts", "manifest-metadata.json"),
  manifestPath = join(generatedRoot, "engine-runtime-manifest.json"),
  executeWasmPack = runWasmPack,
} = {}) {
  const root = resolve(repositoryRoot);
  const generated = resolve(generatedRoot);
  const crateDirectory = join(root, "engine", "crates", "jungle-wasm");
  const generatedParent = dirname(generated);
  await mkdir(generatedParent, { recursive: true });
  // Stage on the destination filesystem so the final directory rename remains
  // atomic even when the operating-system temp directory is another mount.
  const temporaryRoot = await mkdtemp(join(generatedParent, ".engine-build-"));
  try {
    const variants = [
      {
        id: "threaded",
        directory: "threaded",
        features: ["simd", "threads"],
        rustFlags: "-C target-feature=+atomics,+bulk-memory,+mutable-globals,+simd128",
        requires: { crossOriginIsolated: true, sharedArrayBuffer: true, simd: true },
      },
      {
        id: "scalar",
        directory: "scalar",
        features: [],
        rustFlags: "",
        requires: {},
      },
    ];

    for (const variant of variants) {
      const outputDirectory = join(temporaryRoot, variant.directory);
      await executeWasmPack({
        repositoryRoot: root,
        crateDirectory,
        outputDirectory,
        features: variant.features,
        rustFlags: variant.rustFlags,
      });
      // Fail early if wasm-pack's expected web-target pair is absent.
      await Promise.all([
        readFile(join(outputDirectory, "jungle_wasm.js")),
        readFile(join(outputDirectory, "jungle_wasm_bg.wasm")),
      ]);
    }

    const stagedWasm = join(temporaryRoot, "published-wasm");
    await mkdir(stagedWasm);
    for (const variant of variants) {
      const destination = join(stagedWasm, variant.directory);
      await mkdir(destination);
      await copyFile(
        join(temporaryRoot, variant.directory, "jungle_wasm.js"),
        join(destination, "jungle_wasm.js"),
      );
      await copyFile(
        join(temporaryRoot, variant.directory, "jungle_wasm_bg.wasm"),
        join(destination, "jungle_wasm_bg.wasm"),
      );
    }
    await replaceDirectory(stagedWasm, join(generated, "jungle-wasm"), generated);

    const stagedTablebases = join(temporaryRoot, "published-tablebase");
    await mkdir(stagedTablebases);
    await copyFile(legacyAssetPath, join(stagedTablebases, "two_piece_v1.jgtb"));
    const optionalNames = new Set(["two_piece_v1.jgtb"]);
    for (const sourcePath of optionalTablebasePaths) {
      const name = sourcePath.split(/[\\/]/).at(-1);
      if (!name || optionalNames.has(name) || !name.endsWith(".jgtbs")) {
        throw new Error(`Optional tablebase paths must have unique .jgtbs names: ${sourcePath}`);
      }
      optionalNames.add(name);
      await copyFile(sourcePath, join(stagedTablebases, name));
    }
    await replaceDirectory(stagedTablebases, join(generated, "tablebase"), generated);
    await removeLegacyPythonArtifacts(generated);

    const artifactMetadata = {
      "jungle-wasm/scalar/jungle_wasm.js": { id: "engine:scalar:module", kind: "engine" },
      "jungle-wasm/scalar/jungle_wasm_bg.wasm": { id: "engine:scalar:wasm", kind: "engine" },
      "jungle-wasm/threaded/jungle_wasm.js": { id: "engine:threaded:module", kind: "engine" },
      "jungle-wasm/threaded/jungle_wasm_bg.wasm": { id: "engine:threaded:wasm", kind: "engine" },
      "tablebase/two_piece_v1.jgtb": { id: "tablebase:two-piece-v1", kind: "tablebase" },
    };
    const runtimeVariants = variants.map((variant) => ({
      id: variant.id,
      moduleArtifactId: `engine:${variant.id}:module`,
      wasmArtifactId: `engine:${variant.id}:wasm`,
      requires: variant.requires,
    }));
    return writeEngineArtifactManifest({
      inputDirectory: generated,
      outputPath: manifestPath,
      urlPrefix: "/generated",
      rulesFingerprint: DEFAULT_RULES_FINGERPRINT,
      metadataPath: strengthMetadataPath,
      artifactMetadata,
      includePrefixes: ["jungle-wasm", "tablebase"],
      variants: runtimeVariants,
    });
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
}

const isMain = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  try {
    const result = await buildEngineWasmArtifacts();
    process.stdout.write(
      `${resolve(DEFAULT_REPOSITORY_ROOT, "public", "generated", "engine-runtime-manifest.json")} ${result.sha256}\n`,
    );
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
