import { describe, expect, it } from "vitest";
import { selectEvictions, STRENGTH_ARTIFACT_CACHE_BYTES } from "./artifactCache";

describe("strength artifact cache budgeting", () => {
  it("keeps cached data when an incoming artifact fits", () => {
    expect(selectEvictions([{ id: "two", size: 10, lastAccessed: 1 }], 20, 40)).toEqual([]);
  });

  it("evicts least-recently-used complete artifacts until the budget fits", () => {
    expect(
      selectEvictions(
        [
          { id: "newest", size: 15, lastAccessed: 30 },
          { id: "oldest", size: 10, lastAccessed: 10 },
          { id: "middle", size: 10, lastAccessed: 20 },
        ],
        25,
        40,
      ),
    ).toEqual(["oldest", "middle"]);
  });

  it("uses the agreed 250 MiB default", () => {
    expect(STRENGTH_ARTIFACT_CACHE_BYTES).toBe(250 * 1024 * 1024);
  });
});
