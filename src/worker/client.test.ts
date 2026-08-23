import { GameWorkerClient, GameWorkerError } from "./client";
import { snapshotFixture } from "../test/fixtures";

type Listener = (event: MessageEvent<unknown> | ErrorEvent) => void;

class MockWorker {
  static instances: MockWorker[] = [];
  readonly postMessage = vi.fn();
  readonly terminate = vi.fn();
  private readonly listeners = new Map<string, Set<Listener>>();

  constructor() {
    MockWorker.instances.push(this);
  }

  addEventListener(type: string, listener: Listener): void {
    const values = this.listeners.get(type) ?? new Set<Listener>();
    values.add(listener);
    this.listeners.set(type, values);
  }

  removeEventListener(type: string, listener: Listener): void {
    this.listeners.get(type)?.delete(listener);
  }

  emitMessage(data: unknown): void {
    for (const listener of this.listeners.get("message") ?? []) listener({ data } as MessageEvent<unknown>);
  }
}

describe("GameWorkerClient", () => {
  beforeEach(() => {
    MockWorker.instances = [];
    vi.stubGlobal("Worker", MockWorker);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("advances revisions and sends the latest revision with each request", async () => {
    const client = new GameWorkerClient();
    const worker = MockWorker.instances[0];
    const pending = client.request("newGame", { difficulty: "medium", humanSide: "blue" });
    expect(worker.postMessage).toHaveBeenLastCalledWith(expect.objectContaining({ id: 1, revision: 0, type: "newGame" }));
    const snapshot = { ...snapshotFixture(), revision: 1 };
    worker.emitMessage({ id: 1, revision: 1, type: "state", payload: snapshot });
    await expect(pending).resolves.toEqual(snapshot);
    client.request("legalTargets", { origin: 42 });
    expect(worker.postMessage).toHaveBeenLastCalledWith(expect.objectContaining({ id: 2, revision: 1 }));
    client.destroy();
  });

  it("rejects a delayed response from an older authoritative revision", async () => {
    const client = new GameWorkerClient();
    const worker = MockWorker.instances[0];
    const stale = client.request("legalTargets", { origin: 42 });
    const newer = client.request("newGame", { difficulty: "hard", humanSide: "red" });
    const snapshot = { ...snapshotFixture(), revision: 1, difficulty: "hard" as const, humanSide: "red" as const };
    worker.emitMessage({ id: 2, revision: 1, type: "state", payload: snapshot });
    await expect(newer).resolves.toEqual(snapshot);
    worker.emitMessage({
      id: 1,
      revision: 0,
      type: "state",
      payload: { origin: 42, piece: null, effectiveRank: 0, targets: [] },
    });
    await expect(stale).rejects.toMatchObject({ code: "STALE_RESPONSE" });
    client.destroy();
  });

  it("ignores unknown thinking messages and rejects malformed known responses", async () => {
    const onThinking = vi.fn();
    const onSearchProgress = vi.fn();
    const onFatalError = vi.fn();
    const client = new GameWorkerClient({ onThinking, onSearchProgress, onFatalError });
    const worker = MockWorker.instances[0];
    const initialization = client.initialize("https://example.test/");
    worker.emitMessage({ id: 1, revision: 0, type: "ready", payload: snapshotFixture() });
    await initialization;
    worker.emitMessage({
      id: 999,
      revision: 0,
      type: "thinking",
      payload: { searchId: 999, baseRevision: 0, positionHash: "0123456789abcdef", message: "stale" },
    });
    expect(onThinking).not.toHaveBeenCalled();

    const pending = client.request("aiMove", { difficulty: "easy" });
    worker.emitMessage({
      id: 2,
      revision: 0,
      type: "thinking",
      payload: { searchId: 2, baseRevision: 0, positionHash: "0123456789abcdef", message: "searching" },
    });
    expect(onThinking).toHaveBeenCalledWith(true);
    worker.emitMessage({
      id: 2,
      revision: 0,
      type: "searchProgress",
      payload: {
        searchId: 2,
        baseRevision: 0,
        positionHash: "0123456789abcdef",
        depth: 3,
        nodes: 128,
        elapsedMs: 10,
      },
    });
    expect(onSearchProgress).toHaveBeenCalledWith(expect.objectContaining({ depth: 3, nodes: 128 }));
    worker.emitMessage({ id: 2, revision: 1, type: "state", payload: { board: [] } });
    await expect(pending).rejects.toBeInstanceOf(GameWorkerError);
    expect(onFatalError).toHaveBeenCalledWith(expect.objectContaining({ code: "INVALID_WORKER_RESPONSE" }));
    client.destroy();
  });

  it("cancels the active search without restarting the authoritative Worker", async () => {
    const client = new GameWorkerClient();
    const worker = MockWorker.instances[0];
    const initialization = client.initialize("https://example.test/");
    worker.emitMessage({ id: 1, revision: 0, type: "ready", payload: snapshotFixture() });
    await initialization;
    const search = client.request("aiMove", { difficulty: "hard", timeLimitMs: 2_000 });
    const cancellation = client.cancelAiSearch();
    expect(worker.postMessage).toHaveBeenLastCalledWith(expect.objectContaining({
      id: 3,
      revision: 0,
      type: "cancelAi",
      payload: { searchId: 2 },
    }));

    worker.emitMessage({
      id: 2,
      revision: 0,
      type: "error",
      payload: { code: "SEARCH_CANCELLED", message: "AI search cancelled.", searchId: 2 },
    });
    worker.emitMessage({ id: 3, revision: 0, type: "state", payload: snapshotFixture() });
    await expect(search).rejects.toMatchObject({ code: "SEARCH_CANCELLED" });
    await expect(cancellation).resolves.toEqual(snapshotFixture());
    expect(worker.terminate).not.toHaveBeenCalled();
    client.destroy();
  });

  it("rejects an AI result whose search identity does not match the requested position", async () => {
    const onFatalError = vi.fn();
    const client = new GameWorkerClient({ onFatalError });
    const worker = MockWorker.instances[0];
    const initialization = client.initialize("https://example.test/");
    worker.emitMessage({ id: 1, revision: 0, type: "ready", payload: snapshotFixture() });
    await initialization;

    const search = client.request("aiMove", { difficulty: "hard", timeLimitMs: 2_000 });
    const result = {
      ...snapshotFixture(),
      revision: 1,
      aiStats: {
        difficulty: "hard" as const,
        searchId: 2,
        baseRevision: 0,
        positionHash: "fedcba9876543210",
        depth: 5,
        nodes: 1_024,
        nodesPerSecond: 10_000,
        transpositionHits: 12,
        tablebaseHits: 0,
        elapsedMs: 100,
        score: 24,
        stopReason: "time" as const,
        principalVariation: [],
      },
    };
    worker.emitMessage({ id: 2, revision: 1, type: "state", payload: result });

    await expect(search).rejects.toMatchObject({ code: "STALE_SEARCH_RESULT" });
    expect(onFatalError).toHaveBeenCalledWith(expect.objectContaining({ code: "STALE_SEARCH_RESULT" }));
    expect(client.currentRevision).toBe(0);
    client.destroy();
  });

  it("recreates the Worker and resets its revision on restart", async () => {
    const client = new GameWorkerClient();
    const first = MockWorker.instances[0];
    const pending = client.request("newGame", { difficulty: "easy", humanSide: "blue" });
    client.restart();
    await expect(pending).rejects.toMatchObject({ code: "WORKER_RESTARTED" });
    expect(first.terminate).toHaveBeenCalledOnce();
    expect(MockWorker.instances).toHaveLength(2);
    expect(client.currentRevision).toBe(0);
    client.destroy();
  });
});
