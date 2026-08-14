export type TerrainKind = "land" | "water" | "blue-trap" | "red-trap" | "blue-den" | "red-den";

function square(row: number, column: number): number {
  return row * 7 + column;
}

export const TERRAIN_INDICES = {
  water: [
    square(3, 1), square(3, 2), square(3, 4), square(3, 5),
    square(4, 1), square(4, 2), square(4, 4), square(4, 5),
    square(5, 1), square(5, 2), square(5, 4), square(5, 5),
  ],
  blueTraps: [square(8, 2), square(8, 4), square(7, 3)],
  redTraps: [square(0, 2), square(0, 4), square(1, 3)],
  blueDen: square(8, 3),
  redDen: square(0, 3),
} as const;

const WATER = new Set<number>(TERRAIN_INDICES.water);
const BLUE_TRAPS = new Set<number>(TERRAIN_INDICES.blueTraps);
const RED_TRAPS = new Set<number>(TERRAIN_INDICES.redTraps);

export function terrainAt(index: number): TerrainKind {
  if (WATER.has(index)) return "water";
  if (index === TERRAIN_INDICES.blueDen) return "blue-den";
  if (index === TERRAIN_INDICES.redDen) return "red-den";
  if (BLUE_TRAPS.has(index)) return "blue-trap";
  if (RED_TRAPS.has(index)) return "red-trap";
  return "land";
}

export function terrainLabel(terrain: TerrainKind): string {
  if (terrain === "water") return "River";
  if (terrain === "blue-den") return "Blue den";
  if (terrain === "red-den") return "Red den";
  if (terrain === "blue-trap") return "Blue trap";
  if (terrain === "red-trap") return "Red trap";
  return "Land";
}
