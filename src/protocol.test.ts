import { isGameSnapshot, isResponsePayloadForRequest, isWorkerRequest, isWorkerResponse } from "./protocol";
import { snapshotFixture } from "./test/fixtures";

describe("worker protocol guards", () => {
  it("accepts valid request and response envelopes", () => {
    const snapshot = snapshotFixture();
    expect(isWorkerRequest({ id: 1, revision: 2, type: "aiMove", payload: { difficulty: "hard" } })).toBe(true);
    expect(isWorkerResponse({ id: 1, revision: 2, type: "state", payload: snapshot })).toBe(true);
  });

  it("rejects malformed and unknown messages", () => {
    expect(isWorkerRequest({ id: "1", revision: 0, type: "aiMove" })).toBe(false);
    expect(isWorkerRequest({ id: 1, revision: 0, type: "teleport" })).toBe(false);
    expect(isWorkerRequest({ id: 1, revision: 0, type: "applyMove", payload: { origin: -1, destination: 9 } })).toBe(false);
    expect(isWorkerRequest({ id: 1, revision: 0, type: "undo", payload: { plies: 0 } })).toBe(false);
    expect(isWorkerResponse({ id: 1, revision: 0, type: "state" })).toBe(false);
    expect(isWorkerResponse({ id: 1, revision: 0, type: "ready", payload: {} })).toBe(false);
    expect(isWorkerResponse(null)).toBe(false);
  });

  it("recognizes a complete 63-square snapshot", () => {
    expect(isGameSnapshot(snapshotFixture())).toBe(true);
    expect(isGameSnapshot({ ...snapshotFixture(), gameState: { board: [] } })).toBe(false);
  });

  it("matches final response types and payloads to their requests", () => {
    const snapshot = snapshotFixture();
    expect(isResponsePayloadForRequest("newGame", { id: 1, revision: 1, type: "state", payload: snapshot })).toBe(true);
    expect(isResponsePayloadForRequest("exportGame", { id: 1, revision: 1, type: "state", payload: snapshot })).toBe(false);
    expect(isResponsePayloadForRequest("legalTargets", {
      id: 1,
      revision: 1,
      type: "state",
      payload: { origin: 42, piece: null, effectiveRank: 0, targets: [] },
    })).toBe(true);
  });
});
