export type Side = "blue" | "red";
export type Difficulty = "easy" | "medium" | "hard";
export type PieceKind =
  | "RAT"
  | "CAT"
  | "DOG"
  | "WOLF"
  | "LEOPARD"
  | "TIGER"
  | "LION"
  | "ELEPHANT";
export type ResultType = "ongoing" | "den_entry" | "capture_all" | "no_legal_moves";
export type SearchStopReason = "completed" | "time" | "nodes" | "cancelled" | "terminal";

export interface PieceData {
  side: Side;
  kind: PieceKind;
}

export interface MoveData {
  origin: number;
  destination: number;
  piece: PieceData;
  captured: PieceData | null;
  is_jump: boolean;
  note: string;
}

export interface GameStateData {
  board: Array<PieceData | null>;
  side_to_move: Side;
  move_history: MoveData[];
  winner: Side | null;
  result: ResultType;
  result_reason: string;
}

export interface HistoryEntry extends MoveData {
  number: number;
  pieceName: string;
  sideName: string;
  capturedName: string | null;
  notation: string;
}

export interface AiStats {
  difficulty: Difficulty;
  searchId: number;
  laneId?: number;
  workerPoolSize?: number;
  participatingLaneIds?: number[];
  baseRevision: number;
  positionHash: string;
  depth: number;
  nodes: number;
  nodesPerSecond: number;
  transpositionHits: number;
  tablebaseHits: number;
  elapsedMs: number;
  score: number;
  stopReason: SearchStopReason;
  principalVariation: MoveData[];
}

export interface EngineCapabilities {
  wasm: boolean;
  scalar: boolean;
  simd: boolean;
  threads: boolean;
  cancellable: boolean;
  searchProgress: boolean;
  sharedMemory: boolean;
  lazySmp: boolean;
  workerPoolSize: number;
}

export interface RuntimeInfo {
  engineName: string;
  engineVersion: string;
  searchVersion: string;
  tablebaseLoaded: boolean;
  capabilities: EngineCapabilities;
  /** Transitional aliases emitted by the Wasm bridge for older consumers. */
  pythonVersion?: string;
  hardAiVersion?: string;
}

export interface GameSnapshot {
  revision: number;
  positionHash: string;
  gameState: GameStateData;
  history: HistoryEntry[];
  lastMove: HistoryEntry | null;
  canUndo: boolean;
  canRedo: boolean;
  difficulty: Difficulty;
  humanSide: Side;
  aiStats: AiStats | null;
  runtime: RuntimeInfo;
  appliedMove?: HistoryEntry;
  appliedPlies?: number;
}

export interface LegalTarget {
  destination: number;
  capture: boolean;
  captured: string | null;
  jump: boolean;
  note: string;
}

export interface LegalTargetsResult {
  origin: number;
  piece: { side: Side; kind: PieceKind; name: string } | null;
  effectiveRank: number;
  targets: LegalTarget[];
}

export interface ExportGameResult {
  gameState: GameStateData;
  difficulty: Difficulty;
  humanSide: Side;
  engineVersion: string;
}

export interface JungleSave {
  schema: "gpt-jungle-save";
  version: 1;
  engineVersion: string;
  createdAt: string;
  gameState: GameStateData;
  settings: {
    difficulty: Difficulty;
    humanSide: Side;
    boardFlipped: boolean;
  };
}

export const PIECE_NAMES: Record<PieceKind, string> = {
  RAT: "Rat",
  CAT: "Cat",
  DOG: "Dog",
  WOLF: "Wolf",
  LEOPARD: "Leopard",
  TIGER: "Tiger",
  LION: "Lion",
  ELEPHANT: "Elephant",
};

export const PIECE_RANKS: Record<PieceKind, number> = {
  RAT: 1,
  CAT: 2,
  DOG: 3,
  WOLF: 4,
  LEOPARD: 5,
  TIGER: 6,
  LION: 7,
  ELEPHANT: 8,
};
