import type { SearchProgressPayload } from "../protocol";
import type { TablebaseArtifactDescriptor, WasmAssetUrls } from "./wasm-runtime";

export type SearchWorkerRequest =
  | { type: "init"; urls: WasmAssetUrls; laneId: number; workerPoolSize: number }
  | { type: "loadTablebases"; loadId: number; artifacts: TablebaseArtifactDescriptor[] }
  | { type: "search"; searchId: number; commandJson: string };

export type SearchWorkerResponse =
  | { type: "ready" }
  | { type: "progress"; searchId: number; payload: SearchProgressPayload }
  | { type: "result"; searchId: number; replyJson: string }
  | { type: "tablebasesLoaded"; loadId: number; loadedArtifactIds: string[]; failedArtifactIds: string[] }
  | { type: "error"; searchId: number | null; message: string };

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function isSearchWorkerRequest(value: unknown): value is SearchWorkerRequest {
  if (!isRecord(value) || typeof value.type !== "string") return false;
  if (value.type === "init") {
    return (
      isRecord(value.urls) &&
      Number.isSafeInteger(value.laneId) &&
      Number(value.laneId) >= 0 &&
      Number.isSafeInteger(value.workerPoolSize) &&
      Number(value.workerPoolSize) >= 1 &&
      typeof value.urls.variant === "string" &&
      isRecord(value.urls.module) &&
      typeof value.urls.module.url === "string" &&
      typeof value.urls.module.sha256 === "string" &&
      Number.isInteger(value.urls.module.bytes) &&
      isRecord(value.urls.wasm) &&
      typeof value.urls.wasm.url === "string" &&
      typeof value.urls.wasm.sha256 === "string" &&
      Number.isInteger(value.urls.wasm.bytes) &&
      isRecord(value.urls.tablebase) &&
      typeof value.urls.tablebase.url === "string" &&
      typeof value.urls.tablebase.sha256 === "string" &&
      Number.isInteger(value.urls.tablebase.bytes)
    );
  }
  if (value.type === "loadTablebases") {
    return Number.isSafeInteger(value.loadId) && Number(value.loadId) > 0 &&
      Array.isArray(value.artifacts) && value.artifacts.every((artifact) => (
      isRecord(artifact) &&
      typeof artifact.id === "string" &&
      typeof artifact.url === "string" &&
      /^[0-9a-f]{64}$/.test(String(artifact.sha256)) &&
      Number.isSafeInteger(artifact.bytes) &&
      Number(artifact.bytes) > 0 &&
      artifact.kind === "tablebase" &&
      typeof artifact.rulesFingerprint === "string" &&
      isRecord(artifact.tablebase) &&
      typeof artifact.tablebase.format === "string"
    ));
  }
  return (
    value.type === "search" &&
    Number.isInteger(value.searchId) &&
    Number(value.searchId) > 0 &&
    typeof value.commandJson === "string"
  );
}

export function isSearchWorkerResponse(value: unknown): value is SearchWorkerResponse {
  if (!isRecord(value) || typeof value.type !== "string") return false;
  if (value.type === "ready") return true;
  if (value.type === "tablebasesLoaded") {
    return (
      Number.isSafeInteger(value.loadId) &&
      Number(value.loadId) > 0 &&
      Array.isArray(value.loadedArtifactIds) &&
      value.loadedArtifactIds.every((id) => typeof id === "string") &&
      Array.isArray(value.failedArtifactIds) &&
      value.failedArtifactIds.every((id) => typeof id === "string")
    );
  }
  if (value.type === "error") {
    return (
      (value.searchId === null || (Number.isInteger(value.searchId) && Number(value.searchId) > 0)) &&
      typeof value.message === "string"
    );
  }
  if (!Number.isInteger(value.searchId) || Number(value.searchId) <= 0) return false;
  if (value.type === "result") return typeof value.replyJson === "string";
  return value.type === "progress" && isRecord(value.payload);
}
