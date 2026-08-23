import {
  STRENGTH_ARTIFACT_CACHE_BYTES,
  StrengthArtifactCache,
} from "../engine/artifactCache";

export interface WasmGameSession {
  initial_snapshot_json(): string;
  handle(commandJson: string): string;
  search(commandJson: string, progressCallback?: (progressJson: string) => void): string;
  cancel_search(searchId?: number): boolean;
  capabilities_json(): string;
  engine_metadata_json(): string;
  configure_runtime_features(featuresJson: string): string;
  configure_search_lane(laneId: number): string;
  load_tablebase(bytes: Uint8Array, descriptorJson: string): string;
  load_tablebase_shard?: (bytes: Uint8Array, descriptorJson: string) => string;
  free?: () => void;
  destroy?: () => void;
}

interface WasmRuntimeModule {
  default: (wasm?: WebAssembly.Module | URL) => Promise<unknown>;
  WasmGameSession: new () => WasmGameSession;
}

interface LoadedWasmRuntime {
  module: WasmRuntimeModule;
  tablebaseBytes: ArrayBuffer;
  tablebase: TablebaseArtifactDescriptor;
}

interface RuntimeArtifactDescriptor {
  id: string;
  url: string;
  sha256: string;
  bytes: number;
  kind: "engine" | "book" | "model" | "tablebase";
  rulesFingerprint: string;
}

interface EngineArtifactDescriptor extends RuntimeArtifactDescriptor {
  kind: "engine";
}

export interface TablebaseArtifactDescriptor extends RuntimeArtifactDescriptor {
  kind: "tablebase";
  tablebase: {
    format: "legacy-two-piece-v1" | "sparse-shard-v1" | string;
    completeMaterial: boolean;
    records: number;
    maxDistance: number;
    payloadSha256: string;
    material?: { id: string; blue: number[]; red: number[] };
  };
}

interface EngineVariant {
  id: "scalar" | "threaded" | string;
  moduleArtifactId: string;
  wasmArtifactId: string;
  requires: Partial<Record<"crossOriginIsolated" | "sharedArrayBuffer" | "simd", boolean>>;
}

interface EngineRuntimeManifest {
  schema: "gpt-jungle-engine-runtime";
  version: 2;
  engineAbi: "jungle-wasm-v2";
  rulesFingerprint: string;
  artifacts: RuntimeArtifactDescriptor[];
  variants: EngineVariant[];
  materials: TablebaseMaterial[];
}

interface TablebaseMaterial {
  signature: { id: string; blue: number[]; red: number[] };
  complete: boolean;
  positionCount: number;
  recordCount: number;
  bytes: number;
  probeValue: number;
  samplePositions: number;
  artifactIds: string[];
}

const strengthArtifactCache = new StrengthArtifactCache();
export const OPTIONAL_TABLEBASE_RESIDENT_BYTES_PER_LANE = 32 * 1024 * 1024;

export interface WasmAssetUrls {
  variant: string;
  module: EngineArtifactDescriptor;
  wasm: EngineArtifactDescriptor;
  tablebase: TablebaseArtifactDescriptor;
}

interface RuntimePlan {
  variants: WasmAssetUrls[];
  optionalTablebaseGroups: OptionalTablebaseGroup[];
  prefetchTablebases: TablebaseArtifactDescriptor[];
}

export interface OptionalTablebaseGroup {
  materialId: string;
  maxPieces: number;
  bytes: number;
  artifacts: TablebaseArtifactDescriptor[];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isRuntimeModule(value: unknown): value is WasmRuntimeModule {
  return (
    isRecord(value) &&
    typeof value.default === "function" &&
    typeof value.WasmGameSession === "function"
  );
}

function isArtifact(value: unknown, rulesFingerprint: string): value is RuntimeArtifactDescriptor {
  if (!isRecord(value)) return false;
  const common = (
    typeof value.id === "string" &&
    typeof value.url === "string" &&
    /^[0-9a-f]{64}$/.test(String(value.sha256)) &&
    Number.isSafeInteger(value.bytes) &&
    Number(value.bytes) > 0 &&
    ["engine", "book", "model", "tablebase"].includes(String(value.kind)) &&
    value.rulesFingerprint === rulesFingerprint
  );
  if (!common || value.kind !== "tablebase") return common;
  if (!isRecord(value.tablebase)) return false;
  const metadata = value.tablebase;
  const material = metadata.material;
  return (
    typeof metadata.format === "string" &&
    typeof metadata.completeMaterial === "boolean" &&
    Number.isSafeInteger(metadata.records) &&
    Number(metadata.records) > 0 &&
    Number.isSafeInteger(metadata.maxDistance) &&
    Number(metadata.maxDistance) >= 0 &&
    /^[0-9a-f]{64}$/.test(String(metadata.payloadSha256)) &&
    (material === undefined || (
      isRecord(material) &&
      typeof material.id === "string" &&
      Array.isArray(material.blue) &&
      material.blue.length === 8 &&
      material.blue.every((count) => Number.isSafeInteger(count) && Number(count) >= 0) &&
      Array.isArray(material.red) &&
      material.red.length === 8 &&
      material.red.every((count) => Number.isSafeInteger(count) && Number(count) >= 0)
    ))
  );
}

function isVariant(value: unknown): value is EngineVariant {
  if (!isRecord(value) || !isRecord(value.requires)) return false;
  return (
    typeof value.id === "string" &&
    typeof value.moduleArtifactId === "string" &&
    typeof value.wasmArtifactId === "string" &&
    Object.entries(value.requires).every(([name, required]) => (
      ["crossOriginIsolated", "sharedArrayBuffer", "simd"].includes(name) &&
      typeof required === "boolean"
    ))
  );
}

export function variantRequirementsSatisfied(
  requires: Partial<Record<"crossOriginIsolated" | "sharedArrayBuffer" | "simd", boolean>>,
  capabilities: Record<"crossOriginIsolated" | "sharedArrayBuffer" | "simd", boolean>,
): boolean {
  return Object.entries(requires).every(([name, required]) => (
    !required || capabilities[name as keyof typeof capabilities]
  ));
}

function isMaterial(value: unknown): value is TablebaseMaterial {
  if (!isRecord(value) || !isRecord(value.signature) || !Array.isArray(value.artifactIds)) return false;
  const signature = value.signature;
  return (
    typeof signature.id === "string" &&
    Array.isArray(signature.blue) &&
    signature.blue.length === 8 &&
    signature.blue.every((count) => Number.isSafeInteger(count) && Number(count) >= 0) &&
    Array.isArray(signature.red) &&
    signature.red.length === 8 &&
    signature.red.every((count) => Number.isSafeInteger(count) && Number(count) >= 0) &&
    typeof value.complete === "boolean" &&
    Number.isSafeInteger(value.positionCount) && Number(value.positionCount) >= 0 &&
    Number.isSafeInteger(value.recordCount) && Number(value.recordCount) >= 0 &&
    Number.isSafeInteger(value.bytes) && Number(value.bytes) > 0 &&
    Number.isFinite(value.probeValue) && Number(value.probeValue) >= 0 &&
    Number.isSafeInteger(value.samplePositions) && Number(value.samplePositions) >= 0 &&
    value.artifactIds.length > 0 &&
    value.artifactIds.every((id) => typeof id === "string")
  );
}

function runtimeManifest(value: unknown): EngineRuntimeManifest {
  if (
    !isRecord(value) ||
    !Array.isArray(value.artifacts) ||
    !Array.isArray(value.variants) ||
    !Array.isArray(value.materials)
  ) {
    throw new Error("The Jungle engine runtime manifest is malformed.");
  }
  if (
    value.schema !== "gpt-jungle-engine-runtime" ||
    value.version !== 2 ||
    value.engineAbi !== "jungle-wasm-v2" ||
    typeof value.rulesFingerprint !== "string"
  ) {
    throw new Error("The Jungle engine runtime manifest uses an unsupported schema or ABI.");
  }
  const rulesFingerprint = value.rulesFingerprint;
  if (
    !/^[0-9a-f]{64}$/.test(rulesFingerprint) ||
    !value.artifacts.every((artifact) => isArtifact(artifact, rulesFingerprint)) ||
    !value.variants.every(isVariant) ||
    !value.materials.every(isMaterial)
  ) {
    throw new Error("The Jungle engine runtime manifest uses an unsupported schema, ABI, or artifact.");
  }
  return value as unknown as EngineRuntimeManifest;
}

export function selectOptionalTablebaseArtifactIds(
  materials: ReadonlyArray<TablebaseMaterial>,
  artifacts: ReadonlyArray<Pick<RuntimeArtifactDescriptor, "id" | "bytes" | "kind">>,
  mandatoryBytes: number,
  capacityBytes = STRENGTH_ARTIFACT_CACHE_BYTES,
): string[] {
  const artifactsById = new Map(artifacts.map((artifact) => [artifact.id, artifact]));
  const candidates = materials.flatMap((material) => {
    if (!material.complete || material.probeValue <= 0) return [];
    const members = material.artifactIds.map((id) => artifactsById.get(id));
    if (
      members.some((artifact) => !artifact || artifact.kind !== "tablebase") ||
      members.some((artifact) => artifact?.id === "tablebase:two-piece-v1")
    ) return [];
    const bytes = members.reduce((total, artifact) => total + (artifact?.bytes ?? 0), 0);
    if (bytes !== material.bytes) return [];
    return [{ material, bytes, valuePerByte: material.probeValue / bytes }];
  }).sort((left, right) => (
    right.valuePerByte - left.valuePerByte ||
    right.material.probeValue - left.material.probeValue ||
    left.material.signature.id.localeCompare(right.material.signature.id, "en")
  ));

  let remaining = Math.max(0, capacityBytes - mandatoryBytes);
  const selected = new Set<string>();
  for (const candidate of candidates) {
    if (
      candidate.bytes > remaining ||
      candidate.material.artifactIds.some((id) => selected.has(id))
    ) continue;
    for (const id of [...candidate.material.artifactIds].sort((left, right) => left.localeCompare(right, "en"))) {
      selected.add(id);
    }
    remaining -= candidate.bytes;
  }
  return [...selected];
}

export function selectResidentOptionalTablebaseGroups(
  materials: ReadonlyArray<TablebaseMaterial>,
  artifacts: ReadonlyArray<RuntimeArtifactDescriptor>,
  residentCapBytes = OPTIONAL_TABLEBASE_RESIDENT_BYTES_PER_LANE,
): OptionalTablebaseGroup[] {
  const artifactsById = new Map(artifacts.map((artifact) => [artifact.id, artifact]));
  return materials.flatMap((material) => {
    if (!material.complete || material.bytes > residentCapBytes) return [];
    const members = material.artifactIds.map((id) => artifactsById.get(id));
    if (
      members.some((artifact) => !artifact || artifact.kind !== "tablebase") ||
      members.some((artifact) => artifact?.id === "tablebase:two-piece-v1")
    ) return [];
    const tablebases = members as TablebaseArtifactDescriptor[];
    if (
      tablebases.reduce((total, artifact) => total + artifact.bytes, 0) !== material.bytes ||
      tablebases.some((artifact) => artifact.tablebase.material?.id !== material.signature.id)
    ) return [];
    const maxPieces = [...material.signature.blue, ...material.signature.red]
      .reduce((total, count) => total + count, 0);
    return [{
      materialId: material.signature.id,
      maxPieces,
      bytes: material.bytes,
      artifacts: tablebases,
    }];
  }).sort((left, right) => left.materialId.localeCompare(right.materialId, "en"));
}

function absoluteDescriptor<T extends RuntimeArtifactDescriptor>(
  descriptor: T,
  assetBaseUrl: string,
): T {
  return { ...descriptor, url: new URL(descriptor.url, assetBaseUrl).href };
}

async function loadRuntimePlan(assetBaseUrl: string): Promise<RuntimePlan> {
  const manifestUrl = new URL("generated/engine-runtime-manifest.json", assetBaseUrl);
  const response = await fetch(manifestUrl, { cache: "no-cache" });
  if (!response.ok) throw new Error(`Could not load the engine runtime manifest (${response.status}).`);
  const manifest = runtimeManifest(await response.json());
  const artifacts = new Map(manifest.artifacts.map((artifact) => [artifact.id, artifact]));
  if (artifacts.size !== manifest.artifacts.length) {
    throw new Error("The Jungle engine runtime manifest contains duplicate artifact ids.");
  }
  const tablebase = artifacts.get("tablebase:two-piece-v1");
  if (!tablebase || tablebase.kind !== "tablebase") {
    throw new Error("The mandatory two-piece tablebase is missing from the runtime manifest.");
  }
  const capabilities = {
    crossOriginIsolated: globalThis.crossOriginIsolated === true,
    sharedArrayBuffer: supportsSharedMemory(),
    simd: supportsSimd(),
  };

  const variants: WasmAssetUrls[] = [];
  for (const variant of manifest.variants) {
    const supported = variantRequirementsSatisfied(variant.requires, capabilities);
    if (!supported) continue;
    const module = artifacts.get(variant.moduleArtifactId);
    const wasm = artifacts.get(variant.wasmArtifactId);
    if (!module || !wasm || module.kind !== "engine" || wasm.kind !== "engine") {
      throw new Error(`Engine variant ${variant.id} references missing engine artifacts.`);
    }
    variants.push({
      variant: variant.id,
      module: absoluteDescriptor(module as EngineArtifactDescriptor, assetBaseUrl),
      wasm: absoluteDescriptor(wasm as EngineArtifactDescriptor, assetBaseUrl),
      tablebase: absoluteDescriptor(tablebase as TablebaseArtifactDescriptor, assetBaseUrl),
    });
  }
  if (variants.length === 0) throw new Error("No compatible Jungle engine variant is available.");
  const prefetchTablebases = selectOptionalTablebaseArtifactIds(
    manifest.materials,
    manifest.artifacts,
    tablebase.bytes,
  ).map((id) => artifacts.get(id)).filter((artifact): artifact is TablebaseArtifactDescriptor => (
    artifact?.kind === "tablebase"
  )).map((artifact) => absoluteDescriptor(artifact, assetBaseUrl));
  const optionalTablebaseGroups = selectResidentOptionalTablebaseGroups(
    manifest.materials,
    manifest.artifacts,
  ).map((group) => ({
    ...group,
    artifacts: group.artifacts.map((artifact) => absoluteDescriptor(artifact, assetBaseUrl)),
  }));
  return { variants, optionalTablebaseGroups, prefetchTablebases };
}

async function digestHex(bytes: ArrayBuffer): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((value) => value.toString(16).padStart(2, "0")).join("");
}

async function verifiedBytes(descriptor: RuntimeArtifactDescriptor): Promise<ArrayBuffer> {
  const response = await fetch(descriptor.url, { cache: "force-cache" });
  if (!response.ok) throw new Error(`Could not load engine artifact ${descriptor.id} (${response.status}).`);
  const bytes = await response.arrayBuffer();
  if (bytes.byteLength !== descriptor.bytes || (await digestHex(bytes)) !== descriptor.sha256) {
    throw new Error(`Engine artifact ${descriptor.id} failed its integrity check.`);
  }
  return bytes;
}

async function verifiedTablebaseBytes(descriptor: TablebaseArtifactDescriptor): Promise<ArrayBuffer> {
  try {
    const bytes = await strengthArtifactCache.getOrFetch(descriptor);
    if (bytes.byteLength !== descriptor.bytes || (await digestHex(bytes)) !== descriptor.sha256) {
      throw new Error(`Cached artifact ${descriptor.id} failed its integrity check.`);
    }
    return bytes;
  } catch {
    // IndexedDB may be unavailable in private/restricted browser contexts. The
    // mandatory tablebase can still start from a freshly verified response.
    return verifiedBytes(descriptor);
  }
}

export async function loadTablebaseArtifact(
  descriptor: TablebaseArtifactDescriptor,
): Promise<ArrayBuffer> {
  return verifiedTablebaseBytes(descriptor);
}

export async function prefetchTablebaseArtifacts(
  descriptors: ReadonlyArray<TablebaseArtifactDescriptor>,
): Promise<string[]> {
  const loaded: string[] = [];
  for (const descriptor of descriptors) {
    try {
      await verifiedTablebaseBytes(descriptor);
      loaded.push(descriptor.id);
    } catch {
      // Optional strength artifacts never affect engine readiness.
    }
  }
  return loaded;
}

export function installTablebaseArtifact(
  session: WasmGameSession,
  descriptor: TablebaseArtifactDescriptor,
  bytes: ArrayBuffer,
): Record<string, unknown> {
  const loader = descriptor.tablebase.format === "sparse-shard-v1" && session.load_tablebase_shard
    ? session.load_tablebase_shard.bind(session)
    : session.load_tablebase.bind(session);
  const parsed: unknown = JSON.parse(loader(new Uint8Array(bytes), JSON.stringify(descriptor)));
  if (!isRecord(parsed) || parsed.ok !== true || parsed.tablebaseLoaded !== true) {
    const message = isRecord(parsed) && isRecord(parsed.error) && typeof parsed.error.message === "string"
      ? parsed.error.message
      : `Tablebase artifact ${descriptor.id} failed validation.`;
    throw new Error(message);
  }
  return parsed;
}

export async function loadWasmRuntime(urls: WasmAssetUrls): Promise<LoadedWasmRuntime> {
  const [moduleBytes, wasmBytes, tablebaseBytes] = await Promise.all([
    verifiedBytes(urls.module),
    verifiedBytes(urls.wasm),
    verifiedTablebaseBytes(urls.tablebase),
  ]);
  const blobUrl = URL.createObjectURL(new Blob([moduleBytes], { type: "text/javascript" }));
  let imported: unknown;
  try {
    imported = await import(/* @vite-ignore */ blobUrl);
  } finally {
    URL.revokeObjectURL(blobUrl);
  }
  if (!isRuntimeModule(imported)) {
    throw new Error("The Jungle engine module does not expose the expected Wasm interface.");
  }
  await imported.default(await WebAssembly.compile(wasmBytes));
  return { module: imported, tablebaseBytes, tablebase: urls.tablebase };
}

export async function loadPreferredWasmRuntime(assetBaseUrl: string): Promise<{
  runtime: LoadedWasmRuntime;
  urls: WasmAssetUrls;
  optionalTablebaseGroups: OptionalTablebaseGroup[];
  prefetchTablebases: TablebaseArtifactDescriptor[];
}> {
  const { variants, optionalTablebaseGroups, prefetchTablebases } = await loadRuntimePlan(assetBaseUrl);
  let lastError: unknown;
  for (const urls of variants) {
    try {
      return {
        runtime: await loadWasmRuntime(urls),
        urls,
        optionalTablebaseGroups,
        prefetchTablebases,
      };
    } catch (reason: unknown) {
      lastError = reason;
    }
  }
  throw lastError instanceof Error ? lastError : new Error("No Jungle engine variant could be initialized.");
}

function supportsSimd(): boolean {
  try {
    return WebAssembly.validate(new Uint8Array([
      0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 123,
      3, 2, 1, 0, 10, 10, 1, 8, 0, 65, 0, 253, 15, 253, 98, 11,
    ]));
  } catch {
    return false;
  }
}

function supportsSharedMemory(): boolean {
  if (globalThis.crossOriginIsolated !== true || typeof SharedArrayBuffer !== "function") return false;
  try {
    void new WebAssembly.Memory({ initial: 1, maximum: 1, shared: true });
    return true;
  } catch {
    return false;
  }
}

export interface WasmSessionRuntimeOptions {
  laneId?: number;
  workerPoolSize?: number;
  lazySmp?: boolean;
}

export function createWasmSession(
  runtime: LoadedWasmRuntime,
  options: WasmSessionRuntimeOptions = {},
): WasmGameSession {
  const session = new runtime.module.WasmGameSession();
  const sharedMemory = supportsSharedMemory();
  const workerPoolSize = Math.max(1, Math.trunc(options.workerPoolSize ?? 1));
  session.configure_runtime_features(JSON.stringify({
    simd: supportsSimd(),
    sharedMemory,
    atomics: sharedMemory && typeof Atomics === "object",
    threads: sharedMemory,
    workerPoolSize,
    lazySmp: options.lazySmp === true && workerPoolSize > 1,
  }));
  const laneId = Math.max(0, Math.trunc(options.laneId ?? 0));
  const laneReply: unknown = JSON.parse(session.configure_search_lane(laneId));
  if (!isRecord(laneReply) || laneReply.ok !== true || laneReply.laneId !== laneId) {
    disposeSession(session);
    throw new Error(`The search engine rejected Lazy-SMP lane ${laneId}.`);
  }
  try {
    installTablebaseArtifact(session, runtime.tablebase, runtime.tablebaseBytes);
  } catch (reason: unknown) {
    disposeSession(session);
    throw reason;
  }
  return session;
}

export function disposeSession(session: WasmGameSession | null): void {
  if (!session) return;
  if (typeof session.destroy === "function") session.destroy();
  else session.free?.();
}
