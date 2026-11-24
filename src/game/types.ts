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
export interface PieceData {
  side: Side;
  kind: PieceKind;
}

export interface LegalTarget {
  destination: number;
  capture: boolean;
  captured: string | null;
  jump: boolean;
  note: string;
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
