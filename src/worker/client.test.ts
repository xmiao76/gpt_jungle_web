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
    const onFatalError = vi.fn();
    const client = new GameWorkerClient({ onThinking, onFatalError });
    const worker = MockWorker.instances[0];
    worker.emitMessage({ id: 999, revision: 0, type: "thinking", payload: { message: "stale" } });
    expect(onThinking).not.toHaveBeenCalled();

    const pending = client.request("aiMove", { difficulty: "easy" });
    worker.emitMessage({ id: 1, revision: 0, type: "thinking", payload: { message: "searching" } });
    expect(onThinking).toHaveBeenCalledWith(true);
    worker.emitMessage({ id: 1, revision: 1, type: "state", payload: { board: [] } });
    await expect(pending).rejects.toBeInstanceOf(GameWorkerError);
    expect(onFatalError).toHaveBeenCalledWith(expect.objectContaining({ code: "INVALID_WORKER_RESPONSE" }));
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
