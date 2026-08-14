import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import type { GameSnapshot } from "./game/types";
import type { WorkerRequest, WorkerResponse } from "./protocol";
import { snapshotFixture } from "./test/fixtures";

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

  emitMessage(data: WorkerResponse): void {
    for (const listener of this.listeners.get("message") ?? []) listener({ data } as MessageEvent<unknown>);
  }
}

function latestRequest(worker: MockWorker): WorkerRequest {
  return worker.postMessage.mock.calls.at(-1)?.[0] as WorkerRequest;
}

async function respond(worker: MockWorker, response: WorkerResponse): Promise<void> {
  await act(async () => worker.emitMessage(response));
}

async function makeReady(worker: MockWorker, snapshot: GameSnapshot = snapshotFixture()): Promise<void> {
  const request = latestRequest(worker);
  await respond(worker, { id: request.id, revision: snapshot.revision, type: "ready", payload: snapshot });
  await screen.findByRole("dialog", { name: "Start a new expedition" });
}

describe("App runtime states and preferences", () => {
  beforeEach(() => {
    MockWorker.instances = [];
    localStorage.clear();
    vi.stubGlobal("Worker", MockWorker);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("shows staged loading feedback and opens the game only after a valid ready snapshot", async () => {
    render(<App />);
    const worker = MockWorker.instances[0];
    expect(screen.getByRole("heading", { name: "Entering the jungle" })).toBeInTheDocument();
    const request = latestRequest(worker);
    await respond(worker, {
      id: request.id,
      revision: 0,
      type: "progress",
      payload: { stage: "tablebase", percent: 84, message: "Opening endgame tablebase..." },
    });
    expect(screen.getByText("Opening endgame tablebase...")).toBeInTheDocument();
    await makeReady(worker);
    expect(screen.getAllByText(/Python 3\.14\.2/)[0]).toBeInTheDocument();
  });

  it("recovers from a fatal initialization error with a fresh Worker", async () => {
    const user = userEvent.setup();
    render(<App />);
    const first = MockWorker.instances[0];
    const request = latestRequest(first);
    await respond(first, {
      id: request.id,
      revision: 0,
      type: "error",
      payload: { code: "INITIALIZATION_FAILED", message: "Runtime unavailable.", fatal: true },
    });
    expect(await screen.findByText("Runtime unavailable.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retry loading" }));
    expect(first.terminate).toHaveBeenCalledOnce();
    const replacement = MockWorker.instances[1];
    await makeReady(replacement);
    expect(screen.getByRole("dialog", { name: "Start a new expedition" })).toBeVisible();
  });

  it("keeps the start dialog usable after a recoverable command error", async () => {
    const user = userEvent.setup();
    render(<App />);
    const worker = MockWorker.instances[0];
    await makeReady(worker);
    await user.click(screen.getByRole("button", { name: "Begin game" }));
    const request = latestRequest(worker);
    await respond(worker, {
      id: request.id,
      revision: 0,
      type: "error",
      payload: { code: "INVALID_DIFFICULTY", message: "Could not start that game." },
    });
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not start that game.");
    expect(screen.getByRole("button", { name: "Begin game" })).toBeEnabled();
  });

  it("persists mute as a local presentation preference", async () => {
    const user = userEvent.setup();
    render(<App />);
    const worker = MockWorker.instances[0];
    await makeReady(worker);
    await user.click(screen.getByRole("button", { name: "Begin game" }));
    const request = latestRequest(worker);
    const started = { ...snapshotFixture(), revision: 1 };
    await respond(worker, { id: request.id, revision: 1, type: "state", payload: started });
    await user.click(await screen.findByRole("button", { name: "Mute" }));
    await waitFor(() => expect(localStorage.getItem("jungle.muted")).toBe("true"));
    expect(screen.getByRole("button", { name: "Unmute" })).toHaveAttribute("aria-pressed", "true");
  });
});
