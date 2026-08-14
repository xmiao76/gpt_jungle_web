import type { GameSnapshot } from "./types";

export function undoPlyCount(snapshot: GameSnapshot, aiVsAi: boolean): number {
  if (aiVsAi) return 1;
  return snapshot.history.length >= 2 && snapshot.gameState.side_to_move === snapshot.humanSide ? 2 : 1;
}

