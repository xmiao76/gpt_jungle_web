import { snapshotFixture } from "../test/fixtures";
import { undoPlyCount } from "./session";

describe("undo policy", () => {
  it("returns human-vs-AI play to the previous human decision", () => {
    const snapshot = snapshotFixture();
    snapshot.history = [
      { origin: 42, destination: 35, piece: { side: "blue", kind: "ELEPHANT" }, captured: null, is_jump: false, note: "", number: 1, pieceName: "Elephant", sideName: "Blue", capturedName: null, notation: "" },
      { origin: 14, destination: 21, piece: { side: "red", kind: "RAT" }, captured: null, is_jump: false, note: "", number: 2, pieceName: "Rat", sideName: "Red", capturedName: null, notation: "" },
    ];
    expect(undoPlyCount(snapshot, false)).toBe(2);
  });

  it("undoes one ply for AI-vs-AI or an incomplete opening turn", () => {
    const snapshot = snapshotFixture();
    expect(undoPlyCount(snapshot, true)).toBe(1);
    expect(undoPlyCount(snapshot, false)).toBe(1);
  });
});

