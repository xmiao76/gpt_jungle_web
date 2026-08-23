import type { GameSnapshot, GameStateData } from "../game/types";

export function emptyState(): GameStateData {
  return {
    board: Array.from({ length: 63 }, () => null),
    side_to_move: "blue",
    move_history: [],
    winner: null,
    result: "ongoing",
    result_reason: "",
  };
}

export function snapshotFixture(): GameSnapshot {
  const gameState = emptyState();
  gameState.board[42] = { side: "blue", kind: "ELEPHANT" };
  gameState.board[14] = { side: "red", kind: "RAT" };
  return {
    revision: 0,
    positionHash: "0123456789abcdef",
    gameState,
    history: [],
    lastMove: null,
    canUndo: false,
    canRedo: false,
    difficulty: "medium",
    humanSide: "blue",
    aiStats: null,
    runtime: {
      engineName: "Jungle Rust Engine",
      engineVersion: "2.0.0",
      searchVersion: "jungle-search-v3",
      tablebaseLoaded: true,
      capabilities: {
        wasm: true,
        scalar: true,
        simd: false,
        threads: false,
        cancellable: true,
        searchProgress: true,
        sharedMemory: false,
        lazySmp: false,
        workerPoolSize: 1,
      },
    },
  };
}
