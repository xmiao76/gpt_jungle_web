import { emptyState } from "../test/fixtures";

describe("game Worker request boundary", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.resetModules();
  });

  it("returns INVALID_REQUEST for a malformed request with a recoverable id", async () => {
    let onMessage: ((event: MessageEvent<unknown>) => void) | null = null;
    const postMessage = vi.fn();
    vi.stubGlobal("self", {
      postMessage,
      addEventListener: vi.fn((type: string, listener: (event: MessageEvent<unknown>) => void) => {
        if (type === "message") onMessage = listener;
      }),
    });

    await import("./game.worker");
    expect(onMessage).not.toBeNull();
    const dispatch = onMessage as unknown as (event: MessageEvent<unknown>) => void;
    const malformed = { ...emptyState(), board: [] };
    dispatch({
      data: {
        id: 41,
        revision: 0,
        type: "importGame",
        payload: { gameState: malformed, difficulty: "medium", humanSide: "blue" },
      },
    } as MessageEvent<unknown>);

    expect(postMessage).toHaveBeenCalledWith({
      id: 41,
      revision: 0,
      type: "error",
      payload: {
        code: "INVALID_REQUEST",
        message: "The game Worker received an invalid request.",
        fatal: false,
      },
    });
  });

  it("sizes the fast pool without taking the last UI core", async () => {
    vi.stubGlobal("self", { postMessage: vi.fn(), addEventListener: vi.fn() });
    const { recommendedSearchLaneCount } = await import("./game.worker");
    expect(recommendedSearchLaneCount("scalar", 32)).toBe(1);
    expect(recommendedSearchLaneCount("threaded", 1)).toBe(1);
    expect(recommendedSearchLaneCount("threaded", 2)).toBe(1);
    expect(recommendedSearchLaneCount("threaded", 4)).toBe(3);
    expect(recommendedSearchLaneCount("threaded", 16)).toBe(4);
  });

  it("merges completed Lazy-SMP lanes by proof, depth, vote, and aggregate work", async () => {
    vi.stubGlobal("self", { postMessage: vi.fn(), addEventListener: vi.fn() });
    const { mergeLaneReplies } = await import("./game.worker");
    const lane = (
      laneId: number,
      depth: number,
      origin: number,
      destination: number,
      score: number,
      nodes: number,
      termination = "hard_deadline",
    ) => JSON.stringify({
      ok: true,
      revision: 7,
      data: {
        searchId: 9,
        baseRevision: 7,
        positionHash: "0123456789abcdef",
        move: { origin, destination },
        stopReason: termination === "proven" ? "completed" : "time",
        aiStats: {
          laneId,
          depth,
          score,
          nodes,
          elapsedMs: 2_000,
          nodesPerSecond: nodes / 2,
          transpositionHits: laneId + 1,
          tablebaseHits: termination === "proven" ? 1 : 0,
          termination,
          diagnostics: { ttHits: laneId + 1, hashfullPerMille: 10 + laneId },
        },
      },
    });

    const merged = JSON.parse(mergeLaneReplies([
      lane(0, 9, 48, 41, 80, 100),
      lane(1, 10, 44, 45, 70, 200),
      lane(2, 10, 44, 45, 72, 300),
    ]));
    expect(merged.data.move).toEqual({ origin: 44, destination: 45 });
    expect(merged.data.aiStats).toMatchObject({
      depth: 10,
      nodes: 600,
      workerPoolSize: 3,
      participatingLaneIds: [0, 1, 2],
      transpositionHits: 6,
    });
    expect(merged.data.aiStats.diagnostics).toMatchObject({ ttHits: 6, hashfullPerMille: 12 });

    const proven = JSON.parse(mergeLaneReplies([
      lane(0, 12, 48, 41, 200, 500),
      lane(1, 1, 28, 21, 98_000, 20, "proven"),
    ]));
    expect(proven.data.move).toEqual({ origin: 28, destination: 21 });
  });
});
