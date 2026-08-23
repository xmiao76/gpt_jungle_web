import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { lstat, mkdir, readFile, readdir, rename, writeFile } from "node:fs/promises";
import { dirname, extname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

export const RUNTIME_MANIFEST_SCHEMA = "gpt-jungle-engine-runtime";
export const RUNTIME_MANIFEST_VERSION = 2;
export const ENGINE_ABI = "jungle-wasm-v2";
export const STRENGTH_METADATA_SCHEMA = "gpt-jungle-strength-metadata";
export const STRENGTH_METADATA_VERSION = 1;
export const MAX_ARTIFACT_BYTES_EXCLUSIVE = 25 * 1024 * 1024;
export const DEFAULT_RULES_FINGERPRINT =
  "bd5e29e5baf50114aa5ce62f7bfaee325a4aabc4ba69e7b983f3ba79c00a0c3a";

const SHARD_MAGIC = Buffer.from([0x4a, 0x47, 0x54, 0x42, 0x53, 0x48, 0x00, 0x00]);
const LEGACY_MAGIC = Buffer.from([0x4a, 0x47, 0x54, 0x42, 0x32, 0x50, 0x00, 0x00]);
const SHARD_HEADER_BYTES = 140;
const LEGACY_HEADER_BYTES = 84;

function validateFingerprint(value) {
  if (!/^[0-9a-f]{64}$/.test(value)) {
    throw new Error(`Rules fingerprint must be 64 lowercase hexadecimal characters: ${value}`);
  }
  return value;
}

function posixPath(value) {
  return value.split(sep).join("/");
}

function joinUrl(prefix, path) {
  return `${prefix.replace(/\/+$/, "")}/${path
    .split("/")
    .map((part) => encodeURIComponent(part))
    .join("/")}`;
}

function classifyArtifact(path, override) {
  if (override) return override;
  const extension = extname(path).toLowerCase();
  if ([".jgtb", ".jgtbs"].includes(extension)) return "tablebase";
  if ([".nnue", ".model"].includes(extension)) return "model";
  if ([".book", ".jgbk"].includes(extension)) return "book";
  if ([".wasm", ".js", ".mjs"].includes(extension)) return "engine";
  throw new Error(`Cannot infer artifact kind for ${path}; add an artifacts metadata override.`);
}

async function sha256File(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest("hex");
}

async function listFiles(root, excluded) {
  const files = [];
  async function visit(directory) {
    const entries = await readdir(directory, { withFileTypes: true });
    entries.sort((left, right) => left.name.localeCompare(right.name, "en"));
    for (const entry of entries) {
      const absolute = join(directory, entry.name);
      if (excluded.has(resolve(absolute))) continue;
      if (entry.isSymbolicLink()) throw new Error(`Artifact trees cannot contain symlinks: ${absolute}`);
      if (entry.isDirectory()) await visit(absolute);
      else if (entry.isFile()) files.push(absolute);
    }
  }
  await visit(root);
  return files;
}

function readU128Hex(buffer, offset) {
  let value = 0n;
  for (let index = offset + 15; index >= offset; index -= 1) {
    value = (value << 8n) | BigInt(buffer[index]);
  }
  return value.toString(16).padStart(32, "0");
}

function materialId(blue, red) {
  const side = (prefix, counts) => {
    let value = prefix;
    counts.forEach((count, index) => {
      if (count !== 0) value += `${index + 1}${count}`;
    });
    return value;
  };
  return `${side("b", blue)}-${side("r", red)}`;
}

function safeNumber(value, label) {
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error(`${label} exceeds JavaScript's safe integer range.`);
  return Number(value);
}

function parseTablebaseHeader(bytes, expectedRules, path) {
  if (bytes.length >= SHARD_HEADER_BYTES && bytes.subarray(0, 8).equals(SHARD_MAGIC)) {
    const version = bytes.readUInt16LE(8);
    const headerBytes = bytes.readUInt16LE(10);
    if (version !== 1 || headerBytes !== SHARD_HEADER_BYTES) {
      throw new Error(`${path} has an unsupported tablebase shard header.`);
    }
    const flags = bytes.readUInt32LE(12);
    if ((flags & ~1) !== 0) throw new Error(`${path} has unsupported tablebase shard flags.`);
    const rulesFingerprint = bytes.subarray(16, 48).toString("hex");
    if (rulesFingerprint !== expectedRules) {
      throw new Error(`${path} rules fingerprint ${rulesFingerprint} does not match ${expectedRules}.`);
    }
    const blue = [...bytes.subarray(48, 56)];
    const red = [...bytes.subarray(56, 64)];
    const records = safeNumber(bytes.readBigUInt64LE(64), `${path} record count`);
    const expectedBytes = SHARD_HEADER_BYTES + records * 18;
    if (bytes.length !== expectedBytes) {
      throw new Error(`${path} has ${bytes.length} bytes; its header requires ${expectedBytes}.`);
    }
    const payloadSha256 = bytes.subarray(108, 140).toString("hex");
    const actualPayloadSha256 = createHash("sha256").update(bytes.subarray(140)).digest("hex");
    if (payloadSha256 !== actualPayloadSha256) throw new Error(`${path} payload checksum does not match.`);
    return {
      format: "sparse-shard-v1",
      material: { id: materialId(blue, red), blue, red },
      completeMaterial: (flags & 1) !== 0,
      records,
      maxDistance: bytes.readUInt16LE(72),
      minKey: readU128Hex(bytes, 76),
      maxKey: readU128Hex(bytes, 92),
      payloadSha256,
    };
  }

  if (bytes.length >= LEGACY_HEADER_BYTES && bytes.subarray(0, 8).equals(LEGACY_MAGIC)) {
    const version = bytes.readUInt16LE(8);
    const entryBytes = bytes.readUInt16LE(10);
    const records = bytes.readUInt32LE(12);
    if (version !== 1 || entryBytes !== 2 || records !== 508_032) {
      throw new Error(`${path} has an unsupported legacy two-piece header.`);
    }
    const rulesFingerprint = bytes.subarray(20, 52).toString("hex");
    if (rulesFingerprint !== expectedRules) {
      throw new Error(`${path} rules fingerprint ${rulesFingerprint} does not match ${expectedRules}.`);
    }
    const expectedBytes = LEGACY_HEADER_BYTES + records * entryBytes;
    if (bytes.length !== expectedBytes) {
      throw new Error(`${path} has ${bytes.length} bytes; its header requires ${expectedBytes}.`);
    }
    const payloadSha256 = bytes.subarray(52, 84).toString("hex");
    const actualPayloadSha256 = createHash("sha256").update(bytes.subarray(84)).digest("hex");
    if (payloadSha256 !== actualPayloadSha256) throw new Error(`${path} payload checksum does not match.`);
    return {
      format: "legacy-two-piece-v1",
      completeMaterial: true,
      records,
      maxDistance: bytes.readUInt32LE(16),
      payloadSha256,
    };
  }

  throw new Error(`${path} is not a recognized Jungle tablebase asset.`);
}

async function readMetadata(path) {
  if (!path) return { artifacts: {}, materials: {} };
  const parsed = JSON.parse(await readFile(path, "utf8"));
  if (
    parsed.schema !== STRENGTH_METADATA_SCHEMA ||
    parsed.version !== STRENGTH_METADATA_VERSION ||
    (parsed.artifacts !== undefined && (typeof parsed.artifacts !== "object" || parsed.artifacts === null)) ||
    (parsed.materials !== undefined && (typeof parsed.materials !== "object" || parsed.materials === null))
  ) {
    throw new Error(`Strength metadata ${path} uses an unsupported schema or version.`);
  }
  return {
    artifacts: parsed.artifacts ?? {},
    materials: parsed.materials ?? {},
  };
}

export async function buildEngineArtifactManifest({
  inputDirectory,
  urlPrefix = "/generated/engine",
  rulesFingerprint = DEFAULT_RULES_FINGERPRINT,
  metadataPath,
  artifactMetadata = {},
  materialMetadata = {},
  includePrefixes = [],
  variants = [],
  excludePaths = [],
}) {
  const root = resolve(inputDirectory);
  if (!(await lstat(root)).isDirectory()) throw new Error(`Artifact input is not a directory: ${root}`);
  const fingerprint = validateFingerprint(rulesFingerprint);
  const fileMetadata = await readMetadata(metadataPath);
  const metadata = {
    artifacts: { ...fileMetadata.artifacts, ...artifactMetadata },
    materials: { ...fileMetadata.materials, ...materialMetadata },
  };
  const excluded = new Set(excludePaths.map((path) => resolve(path)));
  if (metadataPath) excluded.add(resolve(metadataPath));
  const files = await listFiles(root, excluded);
  const artifacts = [];
  const normalizedPrefixes = includePrefixes.map((prefix) =>
    posixPath(prefix).replace(/^\/+|\/+$/g, ""),
  );

  for (const absolute of files) {
    const path = posixPath(relative(root, absolute));
    if (!path || path.startsWith("../") || isAbsolute(path)) throw new Error(`Unsafe artifact path: ${path}`);
    if (
      normalizedPrefixes.length !== 0 &&
      !normalizedPrefixes.some((prefix) => path === prefix || path.startsWith(`${prefix}/`))
    ) {
      continue;
    }
    const stats = await lstat(absolute);
    if (stats.size >= MAX_ARTIFACT_BYTES_EXCLUSIVE) {
      throw new Error(
        `${path} has ${stats.size} bytes; published artifacts must be smaller than ${MAX_ARTIFACT_BYTES_EXCLUSIVE}.`,
      );
    }
    if (stats.size <= 0) throw new Error(`${path} is empty.`);
    const override = metadata.artifacts[path] ?? {};
    const kind = classifyArtifact(path, override.kind);
    if (!new Set(["engine", "book", "model", "tablebase"]).has(kind)) {
      throw new Error(`Artifact ${path} has unsupported kind ${kind}.`);
    }
    const sha256 = await sha256File(absolute);
    const artifact = {
      id: override.id ?? `${kind}:${path}:${sha256.slice(0, 16)}`,
      // The query keeps stable on-disk Wasm glue names while making browser and
      // edge cache keys content-addressed across releases.
      url: override.url ?? `${joinUrl(urlPrefix, path)}?v=${sha256.slice(0, 16)}`,
      sha256,
      bytes: stats.size,
      kind,
      rulesFingerprint: fingerprint,
    };
    if (kind === "tablebase") {
      artifact.tablebase = parseTablebaseHeader(await readFile(absolute), fingerprint, path);
    }
    artifacts.push(artifact);
  }
  artifacts.sort((left, right) => left.id.localeCompare(right.id, "en"));
  const ids = new Set();
  for (const artifact of artifacts) {
    if (!artifact.id || ids.has(artifact.id)) throw new Error(`Artifact id is empty or duplicated: ${artifact.id}`);
    ids.add(artifact.id);
  }
  for (const variant of variants) {
    if (!variant.id || !ids.has(variant.moduleArtifactId) || !ids.has(variant.wasmArtifactId)) {
      throw new Error(`Engine variant ${variant.id ?? "<empty>"} references missing artifacts.`);
    }
  }

  const materialGroups = new Map();
  for (const artifact of artifacts) {
    const material = artifact.tablebase?.material;
    if (!material) continue;
    const group = materialGroups.get(material.id) ?? {
      signature: material,
      artifactIds: [],
      recordCount: 0,
      bytes: 0,
      allSingleShardComplete: true,
    };
    group.artifactIds.push(artifact.id);
    group.recordCount += artifact.tablebase.records;
    group.bytes += artifact.bytes;
    group.allSingleShardComplete &&= artifact.tablebase.completeMaterial;
    materialGroups.set(material.id, group);
  }

  const materials = [...materialGroups.values()]
    .map((group) => {
      group.artifactIds.sort((left, right) => left.localeCompare(right, "en"));
      const measured = metadata.materials[group.signature.id] ?? {};
      const complete = measured.complete ?? (group.artifactIds.length === 1 && group.allSingleShardComplete);
      const positionCount = measured.positionCount ?? (complete ? group.recordCount : 0);
      if (complete && positionCount !== group.recordCount) {
        throw new Error(
          `Complete material ${group.signature.id} has ${group.recordCount} records but declares ${positionCount} positions.`,
        );
      }
      return {
        signature: group.signature,
        complete,
        positionCount,
        recordCount: group.recordCount,
        bytes: group.bytes,
        probeValue: measured.probeValue ?? 0,
        samplePositions: measured.samplePositions ?? 0,
        artifactIds: group.artifactIds,
      };
    })
    .sort((left, right) => left.signature.id.localeCompare(right.signature.id, "en"));

  const manifest = {
    schema: RUNTIME_MANIFEST_SCHEMA,
    version: RUNTIME_MANIFEST_VERSION,
    engineAbi: ENGINE_ABI,
    rulesFingerprint: fingerprint,
    maxArtifactBytesExclusive: MAX_ARTIFACT_BYTES_EXCLUSIVE,
    totalBytes: artifacts.reduce((total, artifact) => total + artifact.bytes, 0),
    artifacts,
    variants,
    materials,
  };
  const json = `${JSON.stringify(manifest, null, 2)}\n`;
  return {
    manifest,
    json,
    sha256: createHash("sha256").update(json).digest("hex"),
  };
}

export async function writeEngineArtifactManifest(options) {
  const output = resolve(options.outputPath);
  const result = await buildEngineArtifactManifest({
    ...options,
    excludePaths: [...(options.excludePaths ?? []), output],
  });
  if (Buffer.byteLength(result.json) >= MAX_ARTIFACT_BYTES_EXCLUSIVE) {
    throw new Error(
      `Runtime manifest has ${Buffer.byteLength(result.json)} bytes; published artifacts must be smaller than ${MAX_ARTIFACT_BYTES_EXCLUSIVE}.`,
    );
  }
  await mkdir(dirname(output), { recursive: true });
  const temporary = `${output}.tmp`;
  await writeFile(temporary, result.json, "utf8");
  await rename(temporary, output);
  return result;
}

function parseArguments(arguments_) {
  const values = {};
  for (let index = 0; index < arguments_.length; index += 2) {
    const option = arguments_[index];
    const value = arguments_[index + 1];
    if (!option?.startsWith("--") || value === undefined) {
      throw new Error(
        "Usage: node scripts/engine-artifact-manifest.mjs --input DIR --output FILE [--url-prefix URL] [--rules-fingerprint HEX] [--metadata FILE]",
      );
    }
    values[option.slice(2)] = value;
  }
  if (!values.input || !values.output) throw new Error("Both --input and --output are required.");
  return values;
}

const isMain = process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  try {
    const arguments_ = parseArguments(process.argv.slice(2));
    const result = await writeEngineArtifactManifest({
      inputDirectory: arguments_.input,
      outputPath: arguments_.output,
      urlPrefix: arguments_["url-prefix"],
      rulesFingerprint: arguments_["rules-fingerprint"] ?? DEFAULT_RULES_FINGERPRINT,
      metadataPath: arguments_.metadata,
    });
    process.stdout.write(`${resolve(arguments_.output)} ${result.sha256}\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  }
}
