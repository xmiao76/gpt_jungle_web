import { TERRAIN_INDICES, terrainAt } from "./terrain";

describe("board terrain", () => {
  it("matches the authoritative v2 rules coordinates", () => {
    expect(TERRAIN_INDICES.blueTraps).toEqual([2, 4, 10]);
    expect(TERRAIN_INDICES.redTraps).toEqual([58, 60, 52]);
    expect(TERRAIN_INDICES.blueDen).toBe(3);
    expect(TERRAIN_INDICES.redDen).toBe(59);
    expect(TERRAIN_INDICES.water).toEqual([22, 23, 25, 26, 29, 30, 32, 33, 36, 37, 39, 40]);
  });

  it("classifies every special square without adding stray terrain", () => {
    const expected = new Map<number, ReturnType<typeof terrainAt>>([
      ...TERRAIN_INDICES.water.map((index) => [index, "water"] as const),
      ...TERRAIN_INDICES.blueTraps.map((index) => [index, "blue-trap"] as const),
      ...TERRAIN_INDICES.redTraps.map((index) => [index, "red-trap"] as const),
      [TERRAIN_INDICES.blueDen, "blue-den"],
      [TERRAIN_INDICES.redDen, "red-den"],
    ]);

    for (let index = 0; index < 63; index += 1) {
      expect(terrainAt(index)).toBe(expected.get(index) ?? "land");
    }
  });
});
