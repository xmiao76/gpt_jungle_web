import { createSave, parseSave } from "./save";
import { snapshotFixture } from "../test/fixtures";

describe("portable save files", () => {
  it("creates and parses the versioned wrapper", () => {
    const snapshot = snapshotFixture();
    const save = createSave(
      {
        gameState: snapshot.gameState,
        difficulty: "hard",
        humanSide: "red",
        engineVersion: "0.1.0",
      },
      true,
    );
    const parsed = parseSave(JSON.stringify(save));
    expect(parsed.schema).toBe("gpt-jungle-save");
    expect(parsed.settings).toEqual({ difficulty: "hard", humanSide: "red", boardFlipped: true });
    expect(parsed.gameState.board).toHaveLength(63);
  });

  it("rejects malformed JSON and unsupported schemas", () => {
    expect(() => parseSave("{")).toThrow("not valid JSON");
    expect(() => parseSave(JSON.stringify({ schema: "other", version: 1 }))).toThrow("not supported");
  });
});

