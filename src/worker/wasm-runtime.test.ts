import {
  selectOptionalTablebaseArtifactIds,
  selectResidentOptionalTablebaseGroups,
  variantRequirementsSatisfied,
  type TablebaseArtifactDescriptor,
} from "./wasm-runtime";

function artifact(id: string, bytes: number, materialId: string): TablebaseArtifactDescriptor {
  return {
    id,
    url: `https://example.test/${id}.jgtbs`,
    sha256: "a".repeat(64),
    bytes,
    kind: "tablebase",
    rulesFingerprint: "b".repeat(64),
    tablebase: {
      format: "sparse-shard-v1",
      completeMaterial: true,
      records: 1,
      maxDistance: 1,
      payloadSha256: "c".repeat(64),
      material: { id: materialId, blue: [1, 0, 0, 0, 0, 0, 0, 0], red: [1, 0, 0, 0, 0, 0, 0, 0] },
    },
  };
}

function material(id: string, bytes: number, probeValue: number, artifactIds: string[], complete = true) {
  return {
    signature: { id, blue: [1, 0, 0, 0, 0, 0, 0, 0], red: [1, 0, 0, 0, 0, 0, 0, 0] },
    complete,
    positionCount: 1,
    recordCount: 1,
    bytes,
    probeValue,
    samplePositions: 1,
    artifactIds,
  };
}

describe("optional tablebase planning", () => {
  it("prefetches complete groups by probe value per byte within the persistent budget", () => {
    const high = artifact("high", 20, "b11-r11");
    const low = artifact("low", 30, "b12-r12");
    const incomplete = artifact("incomplete", 5, "b13-r13");
    expect(selectOptionalTablebaseArtifactIds(
      [
        material("b12-r12", 30, 30, [low.id]),
        material("b11-r11", 20, 80, [high.id]),
        material("b13-r13", 5, 500, [incomplete.id], false),
      ],
      [high, low, incomplete],
      10,
      40,
    )).toEqual(["high"]);
  });

  it("offers only complete matching groups that fit the per-lane resident cap", () => {
    const small = artifact("small", 20, "b11-r11");
    const large = artifact("large", 40, "b12-r12");
    const incomplete = artifact("partial", 10, "b13-r13");
    expect(selectResidentOptionalTablebaseGroups(
      [
        material("b11-r11", 20, 1, [small.id]),
        material("b12-r12", 40, 2, [large.id]),
        material("b13-r13", 10, 3, [incomplete.id], false),
      ],
      [small, large, incomplete],
      32,
    )).toEqual([expect.objectContaining({ materialId: "b11-r11", bytes: 20, artifacts: [small] })]);
  });
});

describe("Wasm variant fallback", () => {
  it("keeps the scalar variant available without isolation or SIMD", () => {
    const unavailable = { crossOriginIsolated: false, sharedArrayBuffer: false, simd: false };
    expect(variantRequirementsSatisfied({}, unavailable)).toBe(true);
    expect(variantRequirementsSatisfied({
      crossOriginIsolated: true,
      sharedArrayBuffer: true,
      simd: true,
    }, unavailable)).toBe(false);
  });
});
