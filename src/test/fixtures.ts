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
    gameState,
    history: [],
    lastMove: null,
    canUndo: false,
    canRedo: false,
    difficulty: "medium",
    humanSide: "blue",
    aiStats: null,
    runtime: { pythonVersion: "3.14.2", engineVersion: "0.1.0", tablebaseLoaded: true },
  };
}

