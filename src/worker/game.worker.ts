/// <reference lib="webworker" />

import { loadPyodide, type PyodideAPI } from "pyodide";
import type { PyProxy } from "pyodide/ffi";
import {
  isWorkerRequest,
  type ProgressPayload,
  type WorkerErrorPayload,
  type WorkerRequest,
  type WorkerResponse,
} from "../protocol";

declare const self: DedicatedWorkerGlobalScope;

const PYODIDE_VERSION = "314.0.3";
const PYODIDE_BASE = `https://cdn.jsdelivr.net/pyodide/v${PYODIDE_VERSION}/full/`;

interface RuntimeManifest {
  schema: "gpt-jungle-python-runtime";
  version: 1;
  file: string;
  sha256: string;
  bytes: number;
  pyodideVersion: string;
  sourceCommit: string;
}

interface BridgeReply {
  ok: boolean;
  revision: number;
  data?: unknown;
  error?: { code: string; message: string };
}

let pyodide: PyodideAPI | null = null;
let session: PyProxy | null = null;
let currentRevision = 0;
let initializing: Promise<unknown> | null = null;

function respond(response: WorkerResponse): void {
  self.postMessage(response);
}

function progress(request: WorkerRequest, payload: ProgressPayload): void {
  respond({ id: request.id, revision: currentRevision, type: "progress", payload });
}

function errorResponse(
  request: WorkerRequest,
  code: string,
  message: string,
  fatal = false,
): void {
  const payload: WorkerErrorPayload = { code, message, fatal };
  respond({ id: request.id, revision: currentRevision, type: "error", payload });
}

function isRuntimeManifest(value: unknown): value is RuntimeManifest {
  if (!value || typeof value !== "object") return false;
  const item = value as Partial<RuntimeManifest>;
  return (
    item.schema === "gpt-jungle-python-runtime" &&
    item.version === 1 &&
    typeof item.file === "string" &&
    typeof item.sha256 === "string" &&
    typeof item.bytes === "number" &&
    item.pyodideVersion === PYODIDE_VERSION
  );
}

async function digestHex(bytes: ArrayBuffer): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((value) => value.toString(16).padStart(2, "0")).join("");
}

async function initialize(request: WorkerRequest): Promise<unknown> {
  const payload = request.payload as { assetBaseUrl?: unknown } | undefined;
  if (!payload || typeof payload.assetBaseUrl !== "string") {
    throw new Error("Initialization requires an absolute assetBaseUrl.");
  }
  if (session) {
    const snapshot = JSON.parse(String(session.initial_snapshot_json())) as unknown;
    respond({ id: request.id, revision: currentRevision, type: "ready", payload: snapshot });
    return snapshot;
  }

  progress(request, { stage: "runtime", percent: 8, message: "Loading Python runtime..." });
  pyodide = await loadPyodide({
    indexURL: PYODIDE_BASE,
    packageBaseUrl: PYODIDE_BASE,
  });

  progress(request, { stage: "engine", percent: 58, message: "Loading Jungle engine..." });
  const manifestUrl = new URL("generated/python-runtime.json", payload.assetBaseUrl);
  const manifestResponse = await fetch(manifestUrl, { cache: "no-cache" });
  if (!manifestResponse.ok) {
    throw new Error(`Could not load Python runtime manifest (${manifestResponse.status}).`);
  }
  const manifestValue: unknown = await manifestResponse.json();
  if (!isRuntimeManifest(manifestValue)) {
    throw new Error("Python runtime manifest is invalid or uses a different Pyodide version.");
  }
  const archiveResponse = await fetch(new URL(manifestValue.file, manifestUrl));
  if (!archiveResponse.ok) {
    throw new Error(`Could not load Python engine archive (${archiveResponse.status}).`);
  }
  const archive = await archiveResponse.arrayBuffer();
  if (archive.byteLength !== manifestValue.bytes || (await digestHex(archive)) !== manifestValue.sha256) {
    throw new Error("Python engine archive failed its size or SHA-256 integrity check.");
  }
  pyodide.unpackArchive(archive, "zip", { extractDir: "/runtime" });
  pyodide.runPython("import sys\nif '/runtime' not in sys.path: sys.path.insert(0, '/runtime')");

  progress(request, { stage: "tablebase", percent: 84, message: "Opening endgame tablebase..." });
  session = pyodide.runPython("from browser_bridge import create_session\ncreate_session()") as PyProxy;
  const snapshot = JSON.parse(String(session.initial_snapshot_json())) as {
    revision?: number;
    runtime?: { tablebaseLoaded?: boolean };
  };
  if (!snapshot.runtime?.tablebaseLoaded) {
    throw new Error("The exact two-piece tablebase did not load.");
  }
  currentRevision = snapshot.revision ?? 0;
  progress(request, { stage: "ready", percent: 100, message: "Ready to play." });
  respond({ id: request.id, revision: currentRevision, type: "ready", payload: snapshot });
  return snapshot;
}

async function handleRequest(request: WorkerRequest): Promise<void> {
  if (request.type === "init") {
    if (!initializing) {
      initializing = initialize(request).catch((reason: unknown) => {
        const message = reason instanceof Error ? reason.message : String(reason);
        session?.destroy();
        session = null;
        pyodide = null;
        initializing = null;
        errorResponse(request, "INITIALIZATION_FAILED", message, true);
      });
    }
    await initializing;
    return;
  }
  if (!session) {
    errorResponse(request, "NOT_READY", "The Python engine has not finished initializing.");
    return;
  }
  if (request.revision !== currentRevision) {
    errorResponse(
      request,
      "STALE_REVISION",
      `Request revision ${request.revision} does not match current revision ${currentRevision}.`,
    );
    return;
  }
  if (request.type === "aiMove") {
    respond({
      id: request.id,
      revision: currentRevision,
      type: "thinking",
      payload: { message: "Computer is thinking..." },
    });
  }

  try {
    const raw = String(
      session.handle(
        JSON.stringify({
          type: request.type,
          payload: request.payload ?? {},
        }),
      ),
    );
    const bridge = JSON.parse(raw) as BridgeReply;
    if (!bridge.ok) {
      errorResponse(
        request,
        bridge.error?.code ?? "PYTHON_ERROR",
        bridge.error?.message ?? "The Python engine rejected the request.",
      );
      return;
    }
    currentRevision = bridge.revision;
    respond({
      id: request.id,
      revision: currentRevision,
      type: request.type === "exportGame" ? "export" : "state",
      payload: bridge.data,
    });
  } catch (reason: unknown) {
    const message = reason instanceof Error ? reason.message : String(reason);
    errorResponse(request, "PYTHON_ERROR", message);
  }
}

self.addEventListener("message", (event: MessageEvent<unknown>) => {
  if (!isWorkerRequest(event.data)) {
    return;
  }
  void handleRequest(event.data);
});

export {};

