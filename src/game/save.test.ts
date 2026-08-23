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

  it("deeply rejects malformed version-one game state before it reaches the Worker", () => {
    const snapshot = snapshotFixture();
    const save = createSave(
      {
        gameState: snapshot.gameState,
        difficulty: "medium",
        humanSide: "blue",
        engineVersion: "2.0.0",
      },
      false,
    );
    const malformed = {
      ...save,
      gameState: { ...save.gameState, board: save.gameState.board.slice(0, 62) },
    };

    expect(() => parseSave(JSON.stringify(malformed))).toThrow("invalid game state");
  });
});
