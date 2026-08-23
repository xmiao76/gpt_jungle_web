import type {
  Difficulty,
  ExportGameResult,
  GameSnapshot,
  GameStateData,
  LegalTargetsResult,
  MoveData,
  Side,
} from "./game/types";

export type WorkerRequestType =
  | "init"
  | "newGame"
  | "legalTargets"
  | "applyMove"
  | "aiMove"
  | "cancelAi"
  | "undo"
  | "redo"
  | "importGame"
  | "exportGame";

export interface WorkerRequest {
  id: number;
  revision: number;
  type: WorkerRequestType;
  payload?: unknown;
}

export type WorkerResponseType =
  | "progress"
  | "ready"
  | "state"
  | "thinking"
  | "searchProgress"
  | "export"
  | "error";

export interface WorkerResponse {
  id: number;
  revision: number;
  type: WorkerResponseType;
  payload: unknown;
}

export interface WorkerErrorPayload {
  code: string;
  message: string;
  fatal?: boolean;
  searchId?: number;
}

export interface ProgressPayload {
  stage: "runtime" | "engine" | "tablebase" | "ready";
  percent: number;
  message: string;
}

export interface SearchIdentity {
  searchId: number;
  baseRevision: number;
  positionHash: string;
}

export interface ThinkingPayload extends SearchIdentity {
  message: string;
}

export interface SearchProgressPayload extends SearchIdentity {
  depth: number;
  nodes: number;
  elapsedMs: number;
  score?: number;
  principalVariation?: MoveData[];
  message?: string;
}

export interface RequestPayloads {
  init: { assetBaseUrl: string };
  newGame: { difficulty: Difficulty; humanSide: Side };
  legalTargets: { origin: number };
  applyMove: { origin: number; destination: number };
  aiMove: { difficulty: Difficulty; timeLimitMs?: number; nodeLimit?: number };
  cancelAi: { searchId?: number };
  undo: { plies: number };
  redo: { plies: number };
  importGame: { gameState: GameStateData; difficulty: Difficulty; humanSide: Side };
  exportGame: Record<string, never>;
}

export interface ResponsePayloads {
  init: GameSnapshot;
  newGame: GameSnapshot;
  legalTargets: LegalTargetsResult;
  applyMove: GameSnapshot;
  aiMove: GameSnapshot;
  cancelAi: GameSnapshot;
  undo: GameSnapshot;
  redo: GameSnapshot;
  importGame: GameSnapshot;
  exportGame: ExportGameResult;
}

const REQUEST_TYPES = new Set<WorkerRequestType>([
  "init",
  "newGame",
  "legalTargets",
  "applyMove",
  "aiMove",
  "cancelAi",
  "undo",
  "redo",
  "importGame",
  "exportGame",
]);
const RESPONSE_TYPES = new Set<WorkerResponseType>([
  "progress",
  "ready",
  "state",
  "thinking",
  "searchProgress",
  "export",
  "error",
]);
const DIFFICULTIES = new Set<Difficulty>(["easy", "medium", "hard"]);
const SIDES = new Set<Side>(["blue", "red"]);
const PIECE_KINDS = new Set([
  "RAT",
  "CAT",
  "DOG",
  "WOLF",
  "LEOPARD",
  "TIGER",
  "LION",
  "ELEPHANT",
]);
const RESULTS = new Set(["ongoing", "den_entry", "capture_all", "no_legal_moves"]);
const SEARCH_STOP_REASONS = new Set(["completed", "time", "nodes", "cancelled", "terminal"]);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isBoardIndex(value: unknown): value is number {
  return Number.isInteger(value) && Number(value) >= 0 && Number(value) < 63;
}

function isPositionHash(value: unknown): value is string {
  return typeof value === "string" && /^[0-9a-f]{16}$/.test(value);
}

function isSearchIdentity(value: unknown): value is SearchIdentity {
  return (
    isRecord(value) &&
    Number.isInteger(value.searchId) &&
    Number(value.searchId) > 0 &&
    Number.isInteger(value.baseRevision) &&
    Number(value.baseRevision) >= 0 &&
    isPositionHash(value.positionHash)
  );
}

function isThinkingPayload(value: unknown): value is ThinkingPayload {
  return isSearchIdentity(value) && isRecord(value) && typeof value.message === "string";
}

export function isSearchProgressPayload(value: unknown): value is SearchProgressPayload {
  return (
    isSearchIdentity(value) &&
    isRecord(value) &&
    Number.isFinite(value.depth) &&
    Number.isFinite(value.nodes) &&
    Number.isFinite(value.elapsedMs) &&
    (value.score === undefined || Number.isFinite(value.score)) &&
    (value.message === undefined || typeof value.message === "string") &&
    (value.principalVariation === undefined || (
      Array.isArray(value.principalVariation) &&
      value.principalVariation.every(isMove)
    ))
  );
}

function isDifficulty(value: unknown): value is Difficulty {
  return typeof value === "string" && DIFFICULTIES.has(value as Difficulty);
}

function isSide(value: unknown): value is Side {
  return typeof value === "string" && SIDES.has(value as Side);
}

function isPiece(value: unknown): boolean {
  return isRecord(value) && isSide(value.side) && typeof value.kind === "string" && PIECE_KINDS.has(value.kind);
}

function isMove(value: unknown): boolean {
  return (
    isRecord(value) &&
    isBoardIndex(value.origin) &&
    isBoardIndex(value.destination) &&
    isPiece(value.piece) &&
    (value.captured === null || isPiece(value.captured)) &&
    typeof value.is_jump === "boolean" &&
    typeof value.note === "string"
  );
}

function isHistoryEntry(value: unknown): boolean {
  return (
    isMove(value) &&
    isRecord(value) &&
    Number.isInteger(value.number) &&
    typeof value.pieceName === "string" &&
    typeof value.sideName === "string" &&
    (value.capturedName === null || typeof value.capturedName === "string") &&
    typeof value.notation === "string"
  );
}

export function isGameStateData(value: unknown): value is GameStateData {
  if (!isRecord(value) || !Array.isArray(value.board) || value.board.length !== 63) return false;
  return (
    value.board.every((piece) => piece === null || isPiece(piece)) &&
    isSide(value.side_to_move) &&
    Array.isArray(value.move_history) &&
    value.move_history.every(isMove) &&
    (value.winner === null || isSide(value.winner)) &&
    typeof value.result === "string" &&
    RESULTS.has(value.result) &&
    typeof value.result_reason === "string"
  );
}

function isLegalTargetsResult(value: unknown): value is LegalTargetsResult {
  if (!isRecord(value) || !Array.isArray(value.targets)) return false;
  const piece = value.piece;
  return (
    isBoardIndex(value.origin) &&
    (piece === null || (
      isRecord(piece) &&
      isSide(piece.side) &&
      typeof piece.kind === "string" &&
      PIECE_KINDS.has(piece.kind) &&
      typeof piece.name === "string"
    )) &&
    Number.isInteger(value.effectiveRank) &&
    value.targets.every((target) => (
      isRecord(target) &&
      isBoardIndex(target.destination) &&
      typeof target.capture === "boolean" &&
      (target.captured === null || typeof target.captured === "string") &&
      typeof target.jump === "boolean" &&
      typeof target.note === "string"
    ))
  );
}

function isExportGameResult(value: unknown): value is ExportGameResult {
  return (
    isRecord(value) &&
    isGameStateData(value.gameState) &&
    isDifficulty(value.difficulty) &&
    isSide(value.humanSide) &&
    typeof value.engineVersion === "string"
  );
}

function isRequestPayload(type: WorkerRequestType, payload: unknown): boolean {
  if (!isRecord(payload)) return false;
  switch (type) {
    case "init":
      return typeof payload.assetBaseUrl === "string";
    case "newGame":
      return isDifficulty(payload.difficulty) && isSide(payload.humanSide);
    case "legalTargets":
      return isBoardIndex(payload.origin);
    case "applyMove":
      return isBoardIndex(payload.origin) && isBoardIndex(payload.destination);
    case "aiMove":
      return (
        isDifficulty(payload.difficulty) &&
        (payload.timeLimitMs === undefined || (Number.isInteger(payload.timeLimitMs) && Number(payload.timeLimitMs) > 0)) &&
        (payload.nodeLimit === undefined || (Number.isInteger(payload.nodeLimit) && Number(payload.nodeLimit) > 0))
      );
    case "cancelAi":
      return payload.searchId === undefined || (Number.isInteger(payload.searchId) && Number(payload.searchId) > 0);
    case "undo":
    case "redo":
      return Number.isInteger(payload.plies) && Number(payload.plies) > 0;
    case "importGame":
      return isGameStateData(payload.gameState) && isDifficulty(payload.difficulty) && isSide(payload.humanSide);
    case "exportGame":
      return Object.keys(payload).length === 0;
  }
}

export function isWorkerRequest(value: unknown): value is WorkerRequest {
  if (!isRecord(value)) return false;
  return (
    Number.isInteger(value.id) &&
    Number.isInteger(value.revision) &&
    typeof value.type === "string" &&
    REQUEST_TYPES.has(value.type as WorkerRequestType) &&
    isRequestPayload(value.type as WorkerRequestType, value.payload)
  );
}

export function isWorkerResponse(value: unknown): value is WorkerResponse {
  if (!isRecord(value)) return false;
  const envelope = (
    Number.isInteger(value.id) &&
    Number.isInteger(value.revision) &&
    typeof value.type === "string" &&
    RESPONSE_TYPES.has(value.type as WorkerResponseType) &&
    "payload" in value
  );
  if (!envelope) return false;
  if (value.type === "progress") {
    return (
      isRecord(value.payload) &&
      ["runtime", "engine", "tablebase", "ready"].includes(String(value.payload.stage)) &&
      typeof value.payload.percent === "number" &&
      typeof value.payload.message === "string"
    );
  }
  if (value.type === "thinking") {
    return isThinkingPayload(value.payload);
  }
  if (value.type === "searchProgress") {
    return isSearchProgressPayload(value.payload);
  }
  if (value.type === "error") {
    return (
      isRecord(value.payload) &&
      typeof value.payload.code === "string" &&
      typeof value.payload.message === "string" &&
      (value.payload.fatal === undefined || typeof value.payload.fatal === "boolean") &&
      (value.payload.searchId === undefined || (Number.isInteger(value.payload.searchId) && Number(value.payload.searchId) > 0))
    );
  }
  if (value.type === "ready") return isGameSnapshot(value.payload);
  if (value.type === "export") return isExportGameResult(value.payload);
  return isGameSnapshot(value.payload) || isLegalTargetsResult(value.payload);
}

export function isGameSnapshot(value: unknown): value is GameSnapshot {
  if (!isRecord(value) || !isGameStateData(value.gameState) || !Array.isArray(value.history)) return false;
  return (
    Number.isInteger(value.revision) &&
    isPositionHash(value.positionHash) &&
    value.history.every(isHistoryEntry) &&
    (value.lastMove === null || isHistoryEntry(value.lastMove)) &&
    typeof value.canUndo === "boolean" &&
    typeof value.canRedo === "boolean" &&
    isDifficulty(value.difficulty) &&
    isSide(value.humanSide) &&
    (value.aiStats === null || (
      isRecord(value.aiStats) &&
      isDifficulty(value.aiStats.difficulty) &&
      Number.isInteger(value.aiStats.searchId) &&
      Number(value.aiStats.searchId) > 0 &&
      (value.aiStats.laneId === undefined || (
        Number.isSafeInteger(value.aiStats.laneId) && Number(value.aiStats.laneId) >= 0
      )) &&
      (value.aiStats.workerPoolSize === undefined || (
        Number.isSafeInteger(value.aiStats.workerPoolSize) && Number(value.aiStats.workerPoolSize) >= 1
      )) &&
      (value.aiStats.participatingLaneIds === undefined || (
        Array.isArray(value.aiStats.participatingLaneIds) &&
        value.aiStats.participatingLaneIds.every((laneId) => Number.isSafeInteger(laneId) && Number(laneId) >= 0)
      )) &&
      Number.isInteger(value.aiStats.baseRevision) &&
      Number(value.aiStats.baseRevision) >= 0 &&
      isPositionHash(value.aiStats.positionHash) &&
      Number.isFinite(value.aiStats.depth) &&
      Number.isFinite(value.aiStats.nodes) &&
      Number.isFinite(value.aiStats.nodesPerSecond) &&
      Number.isFinite(value.aiStats.transpositionHits) &&
      Number.isFinite(value.aiStats.tablebaseHits) &&
      Number.isFinite(value.aiStats.elapsedMs) &&
      Number.isFinite(value.aiStats.score) &&
      typeof value.aiStats.stopReason === "string" &&
      SEARCH_STOP_REASONS.has(value.aiStats.stopReason) &&
      Array.isArray(value.aiStats.principalVariation) &&
      value.aiStats.principalVariation.every(isMove)
    )) &&
    isRecord(value.runtime) &&
    typeof value.runtime.engineName === "string" &&
    typeof value.runtime.engineVersion === "string" &&
    typeof value.runtime.searchVersion === "string" &&
    typeof value.runtime.tablebaseLoaded === "boolean" &&
    isRecord(value.runtime.capabilities) &&
    typeof value.runtime.capabilities.wasm === "boolean" &&
    typeof value.runtime.capabilities.scalar === "boolean" &&
    typeof value.runtime.capabilities.simd === "boolean" &&
    typeof value.runtime.capabilities.threads === "boolean" &&
    typeof value.runtime.capabilities.cancellable === "boolean" &&
    typeof value.runtime.capabilities.searchProgress === "boolean" &&
    typeof value.runtime.capabilities.sharedMemory === "boolean" &&
    typeof value.runtime.capabilities.lazySmp === "boolean" &&
    Number.isSafeInteger(value.runtime.capabilities.workerPoolSize) &&
    Number(value.runtime.capabilities.workerPoolSize) >= 1 &&
    (value.runtime.pythonVersion === undefined || typeof value.runtime.pythonVersion === "string") &&
    (value.runtime.hardAiVersion === undefined || typeof value.runtime.hardAiVersion === "string") &&
    (value.appliedMove === undefined || isHistoryEntry(value.appliedMove)) &&
    (value.appliedPlies === undefined || Number.isInteger(value.appliedPlies))
  );
}

export function isResponsePayloadForRequest(type: WorkerRequestType, response: WorkerResponse): boolean {
  if (response.type === "error") return true;
  switch (type) {
    case "init":
      return (
        response.type === "ready" &&
        isGameSnapshot(response.payload) &&
        response.payload.revision === response.revision
      );
    case "legalTargets":
      return response.type === "state" && isLegalTargetsResult(response.payload);
    case "exportGame":
      return response.type === "export" && isExportGameResult(response.payload);
    default:
      return (
        response.type === "state" &&
        isGameSnapshot(response.payload) &&
        response.payload.revision === response.revision
      );
  }
}
