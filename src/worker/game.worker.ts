/// <reference lib="webworker" />

import type { GameSnapshot } from "../game/types";
import {
  isGameSnapshot,
  isSearchProgressPayload,
  isWorkerRequest,
  type ProgressPayload,
  type SearchIdentity,
  type SearchProgressPayload,
  type WorkerErrorPayload,
  type WorkerRequest,
  type WorkerResponse,
} from "../protocol";
import {
  isSearchWorkerResponse,
  type SearchWorkerRequest,
} from "./search-protocol";
import {
  disposeSession,
  createWasmSession,
  loadPreferredWasmRuntime,
  prefetchTablebaseArtifacts,
  type OptionalTablebaseGroup,
  type TablebaseArtifactDescriptor,
  type WasmAssetUrls,
  type WasmGameSession,
} from "./wasm-runtime";

declare const self: DedicatedWorkerGlobalScope;

interface BridgeReply {
  ok: boolean;
  revision: number;
  data?: unknown;
  error?: { code: string; message: string; details?: unknown };
}

interface SearchResultData extends SearchIdentity {
  move: { origin: number; destination: number };
  aiStats: unknown;
  stopReason: string;
}

class SearchCancelledError extends Error {
  constructor(readonly searchId: number) {
    super("AI search cancelled.");
    this.name = "SearchCancelledError";
  }
}

interface ActiveSearch {
  searchId: number;
  resolve: (replyJson: string) => void;
  reject: (reason: Error) => void;
  onProgress: (progress: SearchProgressPayload) => void;
}

interface PendingTablebaseLoad {
  artifacts: Map<string, TablebaseArtifactDescriptor>;
  resolve: (loaded: TablebaseArtifactDescriptor[]) => void;
  reject: (reason: Error) => void;
}

class SearchWorkerController {
  private worker: Worker | null = null;
  private initialized = false;
  private ready: Promise<void> = Promise.resolve();
  private resolveReady: (() => void) | null = null;
  private rejectReady: ((reason: Error) => void) | null = null;
  private active: ActiveSearch | null = null;
  private nextLoadId = 1;
  private readonly installedTablebases = new Map<string, TablebaseArtifactDescriptor>();
  private readonly pendingTablebaseLoads = new Map<number, PendingTablebaseLoad>();
  private residentTablebaseGroup: OptionalTablebaseGroup | null = null;
  private preparingTablebase = false;

  constructor(
    private readonly urls: WasmAssetUrls,
    readonly laneId: number,
    private readonly workerPoolSize: number,
  ) {
    this.createWorker();
  }

  get activeSearchId(): number | null {
    return this.active?.searchId ?? null;
  }

  async initialize(): Promise<void> {
    return this.ready;
  }

  get tablebaseGroup(): OptionalTablebaseGroup | null {
    return this.residentTablebaseGroup;
  }

  get availableForSearch(): boolean {
    return this.initialized && !this.active && !this.preparingTablebase;
  }

  async prepareTablebase(group: OptionalTablebaseGroup | null): Promise<boolean> {
    if (this.active) throw new Error("Cannot change tablebases during an AI search.");
    if (this.residentTablebaseGroup?.materialId === group?.materialId) return true;
    if (this.preparingTablebase) return false;
    this.preparingTablebase = true;
    try {
      if (this.residentTablebaseGroup) {
        this.installedTablebases.clear();
        this.residentTablebaseGroup = null;
        this.createWorker();
        await this.ready;
      }
      if (!group) return true;
      const loaded = await this.loadTablebases(group.artifacts);
      if (loaded.length !== group.artifacts.length) {
        this.installedTablebases.clear();
        this.createWorker();
        await this.ready;
        return false;
      }
      this.residentTablebaseGroup = group;
      return true;
    } finally {
      this.preparingTablebase = false;
    }
  }

  private async loadTablebases(artifacts: TablebaseArtifactDescriptor[]): Promise<TablebaseArtifactDescriptor[]> {
    await this.ready;
    const pendingArtifacts = artifacts.filter((artifact) => !this.installedTablebases.has(artifact.id));
    if (pendingArtifacts.length === 0) return [];
    if (!this.worker) throw new Error("The search Worker is not available.");
    const loadId = this.nextLoadId++;
    return new Promise<TablebaseArtifactDescriptor[]>((resolve, reject) => {
      this.pendingTablebaseLoads.set(loadId, {
        artifacts: new Map(pendingArtifacts.map((artifact) => [artifact.id, artifact])),
        resolve,
        reject,
      });
      const message: SearchWorkerRequest = { type: "loadTablebases", loadId, artifacts: pendingArtifacts };
      this.worker?.postMessage(message);
    });
  }

  run(
    searchId: number,
    commandJson: string,
    onProgress: (progress: SearchProgressPayload) => void,
  ): Promise<string> {
    if (this.active) return Promise.reject(new Error("An AI search is already in progress."));

    return new Promise<string>((resolve, reject) => {
      const active: ActiveSearch = { searchId, resolve, reject, onProgress };
      this.active = active;
      void this.ready.then(() => {
        if (this.active !== active || !this.worker) return;
        const message: SearchWorkerRequest = { type: "search", searchId, commandJson };
        this.worker.postMessage(message);
      }).catch((reason: unknown) => {
        if (this.active !== active) return;
        this.active = null;
        reject(reason instanceof Error ? reason : new Error(String(reason)));
      });
    });
  }

  cancel(searchId?: number): boolean {
    const active = this.active;
    if (!active || (searchId !== undefined && active.searchId !== searchId)) return false;
    this.active = null;
    active.reject(new SearchCancelledError(active.searchId));
    this.createWorker();
    return true;
  }

  destroy(): void {
    this.worker?.removeEventListener("message", this.onMessage);
    this.worker?.removeEventListener("error", this.onError);
    this.worker?.terminate();
    this.worker = null;
    this.initialized = false;
    const error = new Error("The search Worker was destroyed.");
    this.rejectReady?.(error);
    this.active?.reject(error);
    this.active = null;
    this.rejectTablebaseLoads(error);
  }

  private createWorker(): void {
    this.worker?.removeEventListener("message", this.onMessage);
    this.worker?.removeEventListener("error", this.onError);
    this.worker?.terminate();
    this.initialized = false;
    this.installedTablebases.clear();
    this.residentTablebaseGroup = null;

    this.ready = new Promise<void>((resolve, reject) => {
      this.resolveReady = resolve;
      this.rejectReady = reject;
    });
    const worker = new Worker(new URL("./search.worker.ts", import.meta.url), { type: "module" });
    worker.addEventListener("message", this.onMessage);
    worker.addEventListener("error", this.onError);
    this.worker = worker;
    const message: SearchWorkerRequest = {
      type: "init",
      urls: this.urls,
      laneId: this.laneId,
      workerPoolSize: this.workerPoolSize,
    };
    worker.postMessage(message);
  }

  private onMessage = (event: MessageEvent<unknown>): void => {
    if (!isSearchWorkerResponse(event.data)) {
      this.failAndRecover(new Error("The search Worker returned a malformed response."));
      return;
    }
    const response = event.data;
    if (response.type === "ready") {
      this.initialized = true;
      this.resolveReady?.();
      this.resolveReady = null;
      this.rejectReady = null;
      return;
    }
    if (response.type === "tablebasesLoaded") {
      const pending = this.pendingTablebaseLoads.get(response.loadId);
      if (!pending) return;
      this.pendingTablebaseLoads.delete(response.loadId);
      const loaded = response.loadedArtifactIds.flatMap((id) => {
        const artifact = pending.artifacts.get(id);
        if (!artifact) return [];
        this.installedTablebases.set(id, artifact);
        return [artifact];
      });
      pending.resolve(loaded);
      return;
    }
    if (response.type === "error") {
      if (
        response.searchId !== null &&
        (!this.active || response.searchId !== this.active.searchId)
      ) return;
      this.failAndRecover(new Error(response.message));
      return;
    }
    const active = this.active;
    if (!active || response.searchId !== active.searchId) return;
    if (response.type === "progress") {
      if (!isSearchProgressPayload(response.payload)) {
        this.failAndRecover(new Error("The search Worker returned invalid progress data."));
        return;
      }
      active.onProgress(response.payload);
      return;
    }
    this.active = null;
    active.resolve(response.replyJson);
  };

  private onError = (event: ErrorEvent): void => {
    this.failAndRecover(new Error(event.message || "The search Worker stopped unexpectedly."));
  };

  private failAndRecover(error: Error): void {
    const shouldRecover = this.initialized;
    this.fail(error);
    if (shouldRecover) this.createWorker();
  }

  private fail(error: Error): void {
    this.rejectReady?.(error);
    this.resolveReady = null;
    this.rejectReady = null;
    const active = this.active;
    this.active = null;
    active?.reject(error);
    this.rejectTablebaseLoads(error);
  }

  private rejectTablebaseLoads(error: Error): void {
    for (const pending of this.pendingTablebaseLoads.values()) pending.reject(error);
    this.pendingTablebaseLoads.clear();
  }
}

interface LaneCandidate {
  reply: BridgeReply;
  data: Record<string, unknown>;
  stats: Record<string, unknown>;
  laneId: number;
}

class SearchWorkerPool {
  private controllers: SearchWorkerController[];
  private activeId: number | null = null;
  private readonly cancelled = new Set<number>();

  constructor(private readonly urls: WasmAssetUrls) {
    const requestedPoolSize = recommendedSearchLaneCount(
      urls.variant,
      self.navigator.hardwareConcurrency,
    );
    this.controllers = Array.from(
      { length: requestedPoolSize },
      (_, laneId) => new SearchWorkerController(urls, laneId, requestedPoolSize),
    );
  }

  get activeSearchId(): number | null {
    return this.activeId;
  }

  get size(): number {
    return this.controllers.length;
  }

  async initialize(): Promise<void> {
    const primary = this.controllers[0];
    await primary.initialize();
    const auxiliaries = await Promise.allSettled(
      this.controllers.slice(1).map((controller) => controller.initialize()),
    );
    this.controllers = [
      primary,
      ...this.controllers.slice(1).filter((controller, index) => {
        const ready = auxiliaries[index]?.status === "fulfilled";
        if (!ready) controller.destroy();
        return ready;
      }),
    ];
  }

  scheduleTablebase(group: OptionalTablebaseGroup | null): void {
    if (!group || this.activeId !== null) return;
    for (const controller of this.controllers.slice(1)) {
      if (controller.tablebaseGroup?.materialId === group.materialId) continue;
      void controller.prepareTablebase(group).catch(() => {
        // Optional exact coverage is opportunistic; lane zero remains ready.
      });
    }
  }

  async run(
    searchId: number,
    commandJson: string,
    fixedNodeMode: boolean,
    onProgress: (progress: SearchProgressPayload) => void,
  ): Promise<string> {
    if (this.activeId !== null) throw new Error("An AI search is already in progress.");
    this.activeId = searchId;
    const lanes = fixedNodeMode
      ? [this.controllers[0]]
      : [
          this.controllers[0],
          ...this.controllers.slice(1).filter((controller) => controller.availableForSearch),
        ];
    const latestProgress = new Map<number, SearchProgressPayload>();
    try {
      const results = await Promise.allSettled(lanes.map((controller) => controller.run(
        searchId,
        commandJson,
        (progressPayload) => {
          latestProgress.set(controller.laneId, progressPayload);
          const progressValues = [...latestProgress.values()];
          const deepest = progressValues.reduce((best, current) => (
            current.depth > best.depth ||
            (current.depth === best.depth && current.score !== undefined && (
              best.score === undefined || current.score > best.score
            ))
              ? current
              : best
          ));
          onProgress({
            ...deepest,
            nodes: progressValues.reduce((total, current) => total + current.nodes, 0),
            elapsedMs: Math.max(...progressValues.map((current) => current.elapsedMs)),
          });
        },
      )));
      if (this.cancelled.delete(searchId)) throw new SearchCancelledError(searchId);
      const replies = results.flatMap((result) => result.status === "fulfilled" ? [result.value] : []);
      if (replies.length === 0) {
        const firstFailure = results.find((result) => result.status === "rejected");
        throw firstFailure?.status === "rejected" && firstFailure.reason instanceof Error
          ? firstFailure.reason
          : new Error("Every parallel search lane failed.");
      }
      return mergeLaneReplies(replies);
    } finally {
      if (this.activeId === searchId) this.activeId = null;
    }
  }

  cancel(searchId?: number): boolean {
    const activeId = this.activeId;
    if (activeId === null || (searchId !== undefined && searchId !== activeId)) return false;
    this.cancelled.add(activeId);
    for (const controller of this.controllers) controller.cancel(activeId);
    return true;
  }

  destroy(): void {
    for (const controller of this.controllers) controller.destroy();
    this.controllers = [];
    this.activeId = null;
    this.cancelled.clear();
  }
}

export function recommendedSearchLaneCount(variant: string, hardwareConcurrency: number): number {
  const cores = Math.max(1, Math.trunc(hardwareConcurrency || 1));
  return variant === "threaded" ? Math.min(4, Math.max(1, cores - 1)) : 1;
}

export function mergeLaneReplies(rawReplies: string[]): string {
  const candidates = rawReplies.flatMap((raw): LaneCandidate[] => {
    const reply = parseBridgeReply(raw);
    if (!reply.ok || !isRecord(reply.data) || !isRecord(reply.data.aiStats)) return [];
    const stats = reply.data.aiStats;
    const laneId = Number(stats.laneId);
    if (
      !Number.isSafeInteger(laneId) || laneId < 0 ||
      !Number.isFinite(stats.depth) || !Number.isFinite(stats.score) ||
      !isRecord(reply.data.move)
    ) return [];
    return [{ reply, data: reply.data, stats, laneId }];
  });
  if (candidates.length === 0) {
    const first = parseBridgeReply(rawReplies[0]);
    return JSON.stringify(first);
  }

  const proven = candidates.filter((candidate) => candidate.stats.termination === "proven");
  const eligible = proven.length > 0 ? proven : candidates;
  const maximumDepth = Math.max(...eligible.map((candidate) => Number(candidate.stats.depth)));
  const deepest = eligible.filter((candidate) => Number(candidate.stats.depth) === maximumDepth);
  const votes = new Map<string, number>();
  for (const candidate of deepest) {
    const movement = candidate.data.move as Record<string, unknown>;
    const key = `${movement.origin}:${movement.destination}`;
    votes.set(key, (votes.get(key) ?? 0) + 1);
  }
  deepest.sort((left, right) => {
    const leftMove = left.data.move as Record<string, unknown>;
    const rightMove = right.data.move as Record<string, unknown>;
    const leftKey = `${leftMove.origin}:${leftMove.destination}`;
    const rightKey = `${rightMove.origin}:${rightMove.destination}`;
    return (
      (votes.get(rightKey) ?? 0) - (votes.get(leftKey) ?? 0) ||
      Number(right.stats.score) - Number(left.stats.score) ||
      Number(leftMove.origin) - Number(rightMove.origin) ||
      Number(leftMove.destination) - Number(rightMove.destination) ||
      left.laneId - right.laneId
    );
  });
  const selected = deepest[0];
  const elapsedMs = Math.max(...candidates.map((candidate) => Number(candidate.stats.elapsedMs) || 0));
  const nodes = candidates.reduce((total, candidate) => total + (Number(candidate.stats.nodes) || 0), 0);
  const aggregate = (field: string) => candidates.reduce(
    (total, candidate) => total + (Number(candidate.stats[field]) || 0),
    0,
  );
  const diagnostics = mergeDiagnostics(candidates.map((candidate) => candidate.stats.diagnostics));
  const aiStats = {
    ...selected.stats,
    nodes,
    elapsedMs,
    nodesPerSecond: elapsedMs > 0 ? Math.round(nodes * 1_000 / elapsedMs) : 0,
    transpositionHits: aggregate("transpositionHits"),
    tablebaseHits: aggregate("tablebaseHits"),
    workerPoolSize: candidates.length,
    participatingLaneIds: candidates.map((candidate) => candidate.laneId).sort((left, right) => left - right),
    diagnostics,
  };
  return JSON.stringify({
    ...selected.reply,
    data: {
      ...selected.data,
      aiStats,
    },
  });
}

function mergeDiagnostics(values: unknown[]): Record<string, number> | undefined {
  const records = values.filter(isRecord);
  if (records.length === 0) return undefined;
  const merged: Record<string, number> = {};
  for (const record of records) {
    for (const [key, value] of Object.entries(record)) {
      if (!Number.isFinite(value)) continue;
      merged[key] = key === "hashfullPerMille"
        ? Math.max(merged[key] ?? 0, Number(value))
        : (merged[key] ?? 0) + Number(value);
    }
  }
  return merged;
}

let session: WasmGameSession | null = null;
let searchWorker: SearchWorkerPool | null = null;
let currentRevision = 0;
let initializing: Promise<GameSnapshot> | null = null;
let optionalTablebaseGroups = new Map<string, OptionalTablebaseGroup>();
let optionalPrefetch: TablebaseArtifactDescriptor[] = [];
let activeAiSearchId: number | null = null;
const cancelledBeforeSearch = new Set<number>();

const PIECE_INDEX = new Map([
  ["RAT", 0],
  ["CAT", 1],
  ["DOG", 2],
  ["WOLF", 3],
  ["LEOPARD", 4],
  ["TIGER", 5],
  ["LION", 6],
  ["ELEPHANT", 7],
]);

function materialId(snapshot: GameSnapshot): string {
  const counts = { blue: Array<number>(8).fill(0), red: Array<number>(8).fill(0) };
  for (const piece of snapshot.gameState.board) {
    if (!piece) continue;
    const index = PIECE_INDEX.get(piece.kind);
    if (index !== undefined) counts[piece.side][index] += 1;
  }
  const sideId = (prefix: string, values: number[]) => values.reduce(
    (id, count, index) => count === 0 ? id : `${id}${index + 1}${count}`,
    prefix,
  );
  return `${sideId("b", counts.blue)}-${sideId("r", counts.red)}`;
}

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
  searchId?: number,
): void {
  const payload: WorkerErrorPayload = { code, message, fatal, ...(searchId === undefined ? {} : { searchId }) };
  respond({ id: request.id, revision: currentRevision, type: "error", payload });
}

function parseBridgeReply(raw: string): BridgeReply {
  const value: unknown = JSON.parse(raw);
  if (!value || typeof value !== "object") throw new Error("The engine returned a malformed response.");
  const reply = value as Partial<BridgeReply>;
  if (typeof reply.ok !== "boolean" || !Number.isInteger(reply.revision)) {
    throw new Error("The engine returned an invalid response envelope.");
  }
  return reply as BridgeReply;
}

function readSnapshot(): GameSnapshot {
  if (!session) throw new Error("The Jungle engine is not ready.");
  const value: unknown = JSON.parse(String(session.initial_snapshot_json()));
  if (!isGameSnapshot(value)) throw new Error("The Jungle engine returned an invalid snapshot.");
  return value;
}

function engineHandle(type: string, payload: unknown): BridgeReply {
  if (!session) throw new Error("The Jungle engine is not ready.");
  return parseBridgeReply(String(session.handle(JSON.stringify({ type, payload }))));
}

async function initializeEngine(request: WorkerRequest): Promise<GameSnapshot> {
  const payload = request.payload as { assetBaseUrl?: unknown } | undefined;
  if (!payload || typeof payload.assetBaseUrl !== "string") {
    throw new Error("Initialization requires an absolute assetBaseUrl.");
  }

  progress(request, { stage: "runtime", percent: 8, message: "Loading WebAssembly runtime..." });
  const loaded = await loadPreferredWasmRuntime(payload.assetBaseUrl);
  const { runtime, urls, optionalTablebaseGroups: groups, prefetchTablebases } = loaded;
  optionalTablebaseGroups = new Map(groups.map((group) => [group.materialId, group]));
  optionalPrefetch = prefetchTablebases;

  progress(request, { stage: "engine", percent: 58, message: "Loading Jungle engine..." });
  searchWorker?.destroy();
  searchWorker = new SearchWorkerPool(urls);
  await searchWorker.initialize();
  disposeSession(session);
  session = createWasmSession(runtime, {
    workerPoolSize: searchWorker.size,
    lazySmp: searchWorker.size > 1,
  });
  const snapshot = readSnapshot();
  currentRevision = snapshot.revision;

  progress(request, { stage: "tablebase", percent: 84, message: "Opening endgame tablebase..." });
  if (!snapshot.runtime.tablebaseLoaded) throw new Error("The exact endgame tablebase did not load.");

  progress(request, { stage: "ready", percent: 100, message: "Ready to play." });
  searchWorker.scheduleTablebase(optionalTablebaseGroups.get(materialId(snapshot)) ?? null);
  return snapshot;
}

function beginOptionalPrefetch(): void {
  const artifacts = optionalPrefetch;
  optionalPrefetch = [];
  if (artifacts.length > 0) void prefetchTablebaseArtifacts(artifacts);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isSearchResultData(value: unknown, identity: SearchIdentity): value is SearchResultData {
  if (!isRecord(value) || !isRecord(value.move)) return false;
  return (
    value.searchId === identity.searchId &&
    value.baseRevision === identity.baseRevision &&
    value.positionHash === identity.positionHash &&
    Number.isInteger(value.move.origin) &&
    Number.isInteger(value.move.destination) &&
    "aiStats" in value &&
    typeof value.stopReason === "string"
  );
}

async function handleAiMove(request: WorkerRequest): Promise<void> {
  if (!searchWorker) {
    errorResponse(request, "NOT_READY", "The search engine has not finished initializing.");
    return;
  }
  if (activeAiSearchId !== null) {
    errorResponse(request, "SEARCH_IN_PROGRESS", "An AI search is already in progress.");
    return;
  }

  const snapshot = readSnapshot();
  const identity: SearchIdentity = {
    searchId: request.id,
    baseRevision: currentRevision,
    positionHash: snapshot.positionHash,
  };
  activeAiSearchId = identity.searchId;
  const requested = request.payload as Record<string, unknown>;
  const searchPayload = {
    ...requested,
    timeLimitMs: requested.timeLimitMs ?? (requested.difficulty === "hard" ? 2_000 : undefined),
    gameState: snapshot.gameState,
    ...identity,
  };

  respond({
    id: request.id,
    revision: currentRevision,
    type: "thinking",
    payload: { ...identity, message: "Computer is thinking..." },
  });

  try {
    const group = optionalTablebaseGroups.get(materialId(snapshot)) ?? null;
    searchWorker.scheduleTablebase(group);
    if (cancelledBeforeSearch.delete(identity.searchId)) {
      throw new SearchCancelledError(identity.searchId);
    }
    const raw = await searchWorker.run(
      identity.searchId,
      JSON.stringify({ type: "search", payload: searchPayload }),
      requested.nodeLimit !== undefined,
      (searchProgress) => {
        if (
          searchProgress.searchId !== identity.searchId ||
          searchProgress.baseRevision !== identity.baseRevision ||
          searchProgress.positionHash !== identity.positionHash
        ) return;
        respond({
          id: request.id,
          revision: identity.baseRevision,
          type: "searchProgress",
          payload: searchProgress,
        });
      },
    );
    const searched = parseBridgeReply(raw);
    if (!searched.ok) {
      errorResponse(
        request,
        searched.error?.code ?? "AI_SEARCH_FAILED",
        searched.error?.message ?? "The search engine rejected the position.",
        false,
        identity.searchId,
      );
      return;
    }
    if (searched.revision !== identity.baseRevision || !isSearchResultData(searched.data, identity)) {
      errorResponse(
        request,
        "STALE_SEARCH_RESULT",
        "The AI result did not match the position it searched.",
        false,
        identity.searchId,
      );
      return;
    }

    const committed = engineHandle("commitAiMove", {
      searchId: identity.searchId,
      baseRevision: identity.baseRevision,
      positionHash: identity.positionHash,
      origin: searched.data.move.origin,
      destination: searched.data.move.destination,
      aiStats: searched.data.aiStats,
      stopReason: searched.data.stopReason,
    });
    if (!committed.ok) {
      errorResponse(
        request,
        committed.error?.code ?? "AI_COMMIT_FAILED",
        committed.error?.message ?? "The AI move could not be applied.",
        false,
        identity.searchId,
      );
      return;
    }
    currentRevision = committed.revision;
    if (isGameSnapshot(committed.data)) {
      searchWorker.scheduleTablebase(
        optionalTablebaseGroups.get(materialId(committed.data)) ?? null,
      );
    }
    respond({ id: request.id, revision: currentRevision, type: "state", payload: committed.data });
  } catch (reason: unknown) {
    if (reason instanceof SearchCancelledError) {
      errorResponse(request, "SEARCH_CANCELLED", reason.message, false, reason.searchId);
      return;
    }
    const message = reason instanceof Error ? reason.message : String(reason);
    errorResponse(request, "AI_SEARCH_FAILED", message, false, identity.searchId);
  } finally {
    cancelledBeforeSearch.delete(identity.searchId);
    if (activeAiSearchId === identity.searchId) activeAiSearchId = null;
  }
}

function handleCancel(request: WorkerRequest): void {
  const payload = request.payload as { searchId?: number };
  const searchId = payload.searchId ?? activeAiSearchId ?? searchWorker?.activeSearchId ?? undefined;
  if (searchId !== undefined) {
    const cancelled = searchWorker?.cancel(searchId) ?? false;
    if (!cancelled && activeAiSearchId === searchId) cancelledBeforeSearch.add(searchId);
    try {
      session?.cancel_search(searchId);
    } catch {
      // The disposable search Worker is the reliable cancellation boundary.
    }
  }
  const snapshot = readSnapshot();
  respond({ id: request.id, revision: currentRevision, type: "state", payload: snapshot });
}

async function handleRequest(request: WorkerRequest): Promise<void> {
  if (request.type === "init") {
    try {
      initializing ??= initializeEngine(request);
      const snapshot = await initializing;
      respond({ id: request.id, revision: currentRevision, type: "ready", payload: snapshot });
      beginOptionalPrefetch();
    } catch (reason: unknown) {
      const message = reason instanceof Error ? reason.message : String(reason);
      searchWorker?.destroy();
      searchWorker = null;
      disposeSession(session);
      session = null;
      initializing = null;
      optionalTablebaseGroups.clear();
      optionalPrefetch = [];
      errorResponse(request, "INITIALIZATION_FAILED", message, true);
    }
    return;
  }
  if (!session) {
    errorResponse(request, "NOT_READY", "The Jungle engine has not finished initializing.");
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
  if (request.type === "cancelAi") {
    handleCancel(request);
    return;
  }
  if (request.type === "aiMove") {
    await handleAiMove(request);
    return;
  }
  if (activeAiSearchId !== null) {
    errorResponse(request, "SEARCH_IN_PROGRESS", "Cancel the active AI search before changing game state.");
    return;
  }

  try {
    const bridge = engineHandle(request.type, request.payload ?? {});
    if (!bridge.ok) {
      errorResponse(
        request,
        bridge.error?.code ?? "ENGINE_ERROR",
        bridge.error?.message ?? "The Jungle engine rejected the request.",
      );
      return;
    }
    currentRevision = bridge.revision;
    if (isGameSnapshot(bridge.data)) {
      searchWorker?.scheduleTablebase(
        optionalTablebaseGroups.get(materialId(bridge.data)) ?? null,
      );
    }
    respond({
      id: request.id,
      revision: currentRevision,
      type: request.type === "exportGame" ? "export" : "state",
      payload: bridge.data,
    });
  } catch (reason: unknown) {
    const message = reason instanceof Error ? reason.message : String(reason);
    errorResponse(request, "ENGINE_ERROR", message);
  }
}

self.addEventListener("message", (event: MessageEvent<unknown>) => {
  if (!isWorkerRequest(event.data)) {
    if (
      isRecord(event.data) &&
      Number.isSafeInteger(event.data.id) &&
      Number(event.data.id) > 0
    ) {
      respond({
        id: Number(event.data.id),
        revision: currentRevision,
        type: "error",
        payload: {
          code: "INVALID_REQUEST",
          message: "The game Worker received an invalid request.",
          fatal: false,
        },
      });
    }
    return;
  }
  void handleRequest(event.data);
});

export {};
