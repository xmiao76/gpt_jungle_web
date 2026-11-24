import type { PieceData } from "../game/types";
export function snapshotFixture() {
  const board = Array<PieceData | null>(63).fill(null);
  board[42] = { side: "red", kind: "ELEPHANT" };
  board[14] = { side: "blue", kind: "RAT" };
  return { gameState: { board }, lastMove: null };
}
