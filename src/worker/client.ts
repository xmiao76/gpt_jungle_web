import {
  isGameSnapshot,
  isResponsePayloadForRequest,
  isWorkerResponse,
  type ProgressPayload,
  type RequestPayloads,
  type ResponsePayloads,
  type SearchProgressPayload,
  type ThinkingPayload,
  type WorkerErrorPayload,
  type WorkerRequest,
  type WorkerRequestType,
} from "../protocol";

interface PendingRequest {
  type: WorkerRequestType;
  revision: number;
  positionHash: string | null;
  resolve: (value: unknown) => void;
  reject: (reason: Error) => void;
}

export class GameWorkerError extends Error {
  readonly code: string;
  readonly fatal: boolean;

  constructor(payload: WorkerErrorPayload) {
    super(payload.message);
    this.name = "GameWorkerError";
    this.code = payload.code;
    this.fatal = payload.fatal ?? false;
  }
}

export interface GameWorkerClientOptions {
  onProgress?: (progress: ProgressPayload) => void;
  onThinking?: (thinking: boolean) => void;
  onSearchProgress?: (progress: SearchProgressPayload) => void;
  onFatalError?: (error: GameWorkerError) => void;
}

export class GameWorkerClient {
  private worker: Worker;
  private nextId = 1;
  private revision = 0;
  private positionHash: string | null = null;
  private activeSearchId: number | null = null;
  private readonly pending = new Map<number, PendingRequest>();
  private readonly options: GameWorkerClientOptions;

  constructor(options: GameWorkerClientOptions = {}) {
    this.options = options;
    this.worker = this.createWorker();
  }

  get currentRevision(): number {
    return this.revision;
  }

  private createWorker(): Worker {
    const worker = new Worker(new URL("./game.worker.ts", import.meta.url), { type: "module" });
    worker.addEventListener("message", this.onMessage);
    worker.addEventListener("error", this.onWorkerError);
    return worker;
  }

  private onMessage = (event: MessageEvent<unknown>): void => {
    if (!isWorkerResponse(event.data)) {
      const candidate = event.data;
      if (candidate && typeof candidate === "object" && "id" in candidate && Number.isInteger(candidate.id)) {
        const pending = this.pending.get(candidate.id as number);
        if (pending) {
          this.pending.delete(candidate.id as number);
          if (pending.type === "aiMove") {
            if (this.activeSearchId === candidate.id) this.activeSearchId = null;
            this.options.onThinking?.(false);
          }
          const error = new GameWorkerError({
            code: "INVALID_WORKER_RESPONSE",
            message: "The game Worker returned a malformed response.",
            fatal: true,
          });
          pending.reject(error);
          this.options.onFatalError?.(error);
        }
      }
      return;
    }
    const response = event.data;
    const pending = this.pending.get(response.id);
    if (response.type === "progress") {
      if (!pending || pending.type !== "init" || response.revision !== this.revision) return;
      this.options.onProgress?.(response.payload as ProgressPayload);
      return;
    }
    if (response.type === "thinking") {
      if (!pending || pending.type !== "aiMove" || response.revision !== this.revision) return;
      const payload = response.payload as ThinkingPayload;
      if (
        payload.searchId !== response.id ||
        payload.baseRevision !== pending.revision ||
        payload.positionHash !== pending.positionHash
      ) return;
      this.options.onThinking?.(true);
      return;
    }
    if (response.type === "searchProgress") {
      if (!pending || pending.type !== "aiMove" || response.revision !== this.revision) return;
      const payload = response.payload as SearchProgressPayload;
      if (
        payload.searchId !== response.id ||
        payload.baseRevision !== pending.revision ||
        payload.positionHash !== pending.positionHash
      ) return;
      this.options.onSearchProgress?.(payload);
      return;
    }
    if (!pending) return;
    this.pending.delete(response.id);
    if (pending.type === "aiMove") {
      if (this.activeSearchId === response.id) this.activeSearchId = null;
      this.options.onThinking?.(false);
    }

    if (response.type === "error") {
      const error = new GameWorkerError(response.payload as WorkerErrorPayload);
      pending.reject(error);
      if (error.fatal) this.options.onFatalError?.(error);
      return;
    }
    if (response.revision < this.revision) {
      pending.reject(new GameWorkerError({
        code: "STALE_RESPONSE",
        message: `Ignored response revision ${response.revision}; current revision is ${this.revision}.`,
      }));
      return;
    }
    if (!isResponsePayloadForRequest(pending.type, response)) {
      const error = new GameWorkerError({
        code: "INVALID_WORKER_RESPONSE",
        message: `The game Worker returned an invalid payload for ${pending.type}.`,
        fatal: true,
      });
      pending.reject(error);
      this.options.onFatalError?.(error);
      return;
    }
    if (pending.type === "aiMove" && isGameSnapshot(response.payload)) {
      const stats = response.payload.aiStats;
      if (
        !stats ||
        stats.searchId !== response.id ||
        stats.baseRevision !== pending.revision ||
        stats.positionHash !== pending.positionHash
      ) {
        const error = new GameWorkerError({
          code: "STALE_SEARCH_RESULT",
          message: "The AI response did not match the position it searched.",
          fatal: true,
        });
        pending.reject(error);
        this.options.onFatalError?.(error);
        return;
      }
    }
    this.revision = response.revision;
    if (isGameSnapshot(response.payload)) this.positionHash = response.payload.positionHash;
    pending.resolve(response.payload);
  };

  private onWorkerError = (event: ErrorEvent): void => {
    const error = new GameWorkerError({
      code: "WORKER_CRASHED",
      message: event.message || "The game Worker stopped unexpectedly.",
      fatal: true,
    });
    for (const pending of this.pending.values()) pending.reject(error);
    this.pending.clear();
    this.activeSearchId = null;
    this.options.onThinking?.(false);
    this.options.onFatalError?.(error);
  };

  async initialize(assetBaseUrl: string): Promise<ResponsePayloads["init"]> {
    return this.request("init", { assetBaseUrl });
  }

  request<K extends WorkerRequestType>(type: K, payload: RequestPayloads[K]): Promise<ResponsePayloads[K]> {
    const id = this.nextId++;
    if (type === "aiMove") {
      if (this.activeSearchId !== null) {
        return Promise.reject(new GameWorkerError({
          code: "SEARCH_IN_PROGRESS",
          message: "An AI search is already in progress.",
        }));
      }
      this.activeSearchId = id;
    }
    const message: WorkerRequest = { id, revision: this.revision, type, payload };
    return new Promise<ResponsePayloads[K]>((resolve, reject) => {
      this.pending.set(id, {
        type,
        revision: this.revision,
        positionHash: this.positionHash,
        resolve: resolve as (value: unknown) => void,
        reject,
      });
      this.worker.postMessage(message);
    });
  }

  async cancelAiSearch(): Promise<ResponsePayloads["cancelAi"] | null> {
    const searchId = this.activeSearchId;
    if (searchId === null) return null;
    return this.request("cancelAi", { searchId });
  }

  restart(): void {
    const error = new GameWorkerError({
      code: "WORKER_RESTARTED",
      message: "The game Worker was restarted.",
      fatal: false,
    });
    for (const pending of this.pending.values()) pending.reject(error);
    this.pending.clear();
    this.activeSearchId = null;
    this.worker.removeEventListener("message", this.onMessage);
    this.worker.removeEventListener("error", this.onWorkerError);
    this.worker.terminate();
    this.revision = 0;
    this.positionHash = null;
    this.worker = this.createWorker();
  }

  destroy(): void {
    this.worker.removeEventListener("message", this.onMessage);
    this.worker.removeEventListener("error", this.onWorkerError);
    this.worker.terminate();
    this.activeSearchId = null;
    this.pending.clear();
  }
}
