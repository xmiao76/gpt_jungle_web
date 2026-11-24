import { apply_move, legal_moves, opening } from "../../generated/standalone";
import type { Side, Difficulty, PieceData, PieceKind } from "./types";
import { PIECE_NAMES } from "./types";

export interface Ply {
  src: number;
  dst: number;
}
export interface Match {
  rev: number;
  board: number[];
  firstSide: Side;
  toPlay: Side;
  history: Ply[];
  noCapturePlies: number;
  status: "playing" | "won" | "drawn" | "forfeited";
  winner: Side | null;
  reason: string;
}
export interface Options {
  firstSide: Side;
  humanSide: Side;
  difficulty: Difficulty;
  auto: boolean;
}
export const kinds = Object.keys(PIECE_NAMES) as PieceKind[];
export const piece = (code: number): PieceData | null =>
  code
    ? { side: code > 8 ? "red" : "blue", kind: kinds[(code - 1) % 8] }
    : null;
export const fresh = (side: Side): Match => JSON.parse(opening(side === "red"));
export const legal = (s: Match): Ply[] =>
  JSON.parse(legal_moves(JSON.stringify(s)));
export const apply = (s: Match, m: Ply): Match =>
  JSON.parse(apply_move(JSON.stringify(s), m.src, m.dst));
export function readSave(text: string) {
  const save = JSON.parse(text);
  if (save.schema !== "gpt-jungle-save")
    throw Error("Unrecognized Jungle save.");
  if (save.version !== 2)
    throw Error(
      "This save belongs to an earlier Jungle ruleset. Open a version-2 save.",
    );
  const o = save.settings as Options;
  if (
    save.rulesId !== "desktop-jungle-v2" ||
    !o ||
    !["red", "blue"].includes(o.firstSide) ||
    !["red", "blue"].includes(o.humanSide) ||
    !["easy", "medium", "hard"].includes(o.difficulty) ||
    typeof o.auto !== "boolean" ||
    typeof save.flipped !== "boolean" ||
    !Array.isArray(save.history) ||
    save.history.length > 10000
  )
    throw Error("Invalid game settings or history.");
  const line = [fresh(o.firstSide)];
  for (const m of save.history) {
    if (
      !m ||
      ![m.src, m.dst].every((v) => Number.isInteger(v) && v >= 0 && v < 63)
    )
      throw Error("Invalid saved coordinates.");
    line.push(apply(line.at(-1)!, m));
  }
  return { line, options: o, flipped: save.flipped as boolean };
}
