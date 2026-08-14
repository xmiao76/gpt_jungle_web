import type { PieceData } from "./game/types";
import type { TerrainKind } from "./game/terrain";

const imported = import.meta.glob<string>("./assets/game/*.png", {
  eager: true,
  query: "?url",
  import: "default",
});

const assets = new Map<string, string>();
for (const [path, url] of Object.entries(imported)) {
  const name = path.split("/").at(-1);
  if (name) assets.set(name, url);
}

function requireAsset(name: string): string {
  const asset = assets.get(name);
  if (!asset) throw new Error(`Missing bundled game asset: ${name}`);
  return asset;
}

export function pieceAsset(piece: PieceData): string {
  return requireAsset(`${piece.side}_${piece.kind.toLowerCase()}.png`);
}

export function terrainAsset(terrain: TerrainKind): string {
  const names: Record<TerrainKind, string> = {
    land: "land_tile.png",
    water: "river_tile.png",
    "blue-trap": "blue_trap_tile.png",
    "red-trap": "red_trap_tile.png",
    "blue-den": "blue_den_tile.png",
    "red-den": "red_den_tile.png",
  };
  return requireAsset(names[terrain]);
}

export const boardBackgroundAsset = requireAsset("board_background.png");

