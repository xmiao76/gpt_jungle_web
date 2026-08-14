export type TerrainKind = "land" | "water" | "blue-trap" | "red-trap" | "blue-den" | "red-den";

const WATER = new Set([22, 23, 25, 26, 29, 30, 32, 33, 36, 37, 39, 40]);
const BLUE_TRAPS = new Set([51, 58, 60]);
const RED_TRAPS = new Set([2, 4, 10]);

export function terrainAt(index: number): TerrainKind {
  if (WATER.has(index)) return "water";
  if (index === 59) return "blue-den";
  if (index === 3) return "red-den";
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

