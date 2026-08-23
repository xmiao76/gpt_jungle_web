/// <reference lib="webworker" />

import type { SearchProgressPayload } from "../protocol";
import {
  isSearchWorkerRequest,
  type SearchWorkerRequest,
  type SearchWorkerResponse,
} from "./search-protocol";
import {
  createWasmSession,
  disposeSession,
  installTablebaseArtifact,
  loadTablebaseArtifact,
  loadWasmRuntime,
  type WasmGameSession,
} from "./wasm-runtime";

declare const self: DedicatedWorkerGlobalScope;

let session: WasmGameSession | null = null;
let operations = Promise.resolve();

function respond(response: SearchWorkerResponse): void {
  self.postMessage(response);
}

function progressValue(raw: unknown): SearchProgressPayload {
  const value: unknown = typeof raw === "string" ? JSON.parse(raw) : raw;
  if (!value || typeof value !== "object") {
    throw new Error("The search engine returned malformed progress data.");
  }
  return value as SearchProgressPayload;
}

async function handleRequest(request: SearchWorkerRequest): Promise<void> {
  if (request.type === "init") {
    const runtime = await loadWasmRuntime(request.urls);
    disposeSession(session);
    session = createWasmSession(runtime, {
      laneId: request.laneId,
      workerPoolSize: request.workerPoolSize,
      lazySmp: request.workerPoolSize > 1,
    });
    respond({ type: "ready" });
    return;
  }

  if (request.type === "loadTablebases") {
    const loadedArtifactIds: string[] = [];
    const failedArtifactIds: string[] = [];
    for (const artifact of request.artifacts) {
      try {
        if (!session) throw new Error("The search engine is not ready.");
        const bytes = await loadTablebaseArtifact(artifact);
        installTablebaseArtifact(session, artifact, bytes);
        loadedArtifactIds.push(artifact.id);
      } catch {
        failedArtifactIds.push(artifact.id);
      }
    }
    respond({ type: "tablebasesLoaded", loadId: request.loadId, loadedArtifactIds, failedArtifactIds });
    return;
  }

  if (!session) {
    respond({ type: "error", searchId: request.searchId, message: "The search engine is not ready." });
    return;
  }

  try {
    const replyJson = session.search(request.commandJson, (raw: string) => {
      respond({
        type: "progress",
        searchId: request.searchId,
        payload: progressValue(raw),
      });
    });
    respond({ type: "result", searchId: request.searchId, replyJson: String(replyJson) });
  } catch (reason: unknown) {
    const message = reason instanceof Error ? reason.message : String(reason);
    respond({ type: "error", searchId: request.searchId, message });
  }
}

self.addEventListener("message", (event: MessageEvent<unknown>) => {
  if (!isSearchWorkerRequest(event.data)) return;
  const request = event.data;
  operations = operations.then(() => handleRequest(request)).catch((reason: unknown) => {
    const message = reason instanceof Error ? reason.message : String(reason);
    respond({ type: "error", searchId: request.type === "search" ? request.searchId : null, message });
  });
});

export {};
