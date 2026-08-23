export const STRENGTH_ARTIFACT_CACHE_BYTES = 250 * 1024 * 1024;

export interface StrengthArtifactDescriptor {
  id: string;
  url: string;
  sha256: string;
  bytes: number;
  kind: "book" | "model" | "tablebase";
  rulesFingerprint: string;
}

interface StoredArtifact {
  id: string;
  sha256: string;
  rulesFingerprint: string;
  bytes: ArrayBuffer;
  size: number;
  lastAccessed: number;
}

const DATABASE_NAME = "gpt-jungle-strength-artifacts";
const DATABASE_VERSION = 1;
const STORE_NAME = "artifacts";

function requestResult<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.addEventListener("success", () => resolve(request.result), { once: true });
    request.addEventListener("error", () => reject(request.error ?? new Error("IndexedDB request failed.")), {
      once: true,
    });
  });
}

function transactionComplete(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.addEventListener("complete", () => resolve(), { once: true });
    transaction.addEventListener("abort", () => reject(transaction.error ?? new Error("Artifact transaction aborted.")), {
      once: true,
    });
    transaction.addEventListener("error", () => reject(transaction.error ?? new Error("Artifact transaction failed.")), {
      once: true,
    });
  });
}

async function sha256Hex(bytes: ArrayBuffer): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((value) => value.toString(16).padStart(2, "0")).join("");
}

export function selectEvictions(
  records: ReadonlyArray<Pick<StoredArtifact, "id" | "size" | "lastAccessed">>,
  incomingBytes: number,
  capacityBytes = STRENGTH_ARTIFACT_CACHE_BYTES,
): string[] {
  const required = records.reduce((total, record) => total + record.size, 0) + incomingBytes - capacityBytes;
  if (required <= 0) return [];

  let reclaimed = 0;
  const selected: string[] = [];
  for (const record of [...records].sort((left, right) => left.lastAccessed - right.lastAccessed)) {
    selected.push(record.id);
    reclaimed += record.size;
    if (reclaimed >= required) break;
  }
  return selected;
}

export class StrengthArtifactCache {
  readonly capacityBytes: number;
  #database: Promise<IDBDatabase> | null = null;

  constructor(capacityBytes = STRENGTH_ARTIFACT_CACHE_BYTES) {
    this.capacityBytes = capacityBytes;
  }

  async getOrFetch(descriptor: StrengthArtifactDescriptor, signal?: AbortSignal): Promise<ArrayBuffer> {
    if (descriptor.bytes <= 0 || descriptor.bytes > this.capacityBytes) {
      throw new Error(`Artifact ${descriptor.id} does not fit the local cache budget.`);
    }
    const cached = await this.#get(descriptor.id);
    if (
      cached &&
      cached.sha256 === descriptor.sha256 &&
      cached.rulesFingerprint === descriptor.rulesFingerprint &&
      cached.size === descriptor.bytes
    ) {
      cached.lastAccessed = Date.now();
      await this.#put(cached);
      return cached.bytes;
    }
    if (cached) await this.#delete(cached.id);

    const response = await fetch(descriptor.url, { cache: "force-cache", signal });
    if (!response.ok) throw new Error(`Could not fetch artifact ${descriptor.id} (${response.status}).`);
    const bytes = await response.arrayBuffer();
    if (bytes.byteLength !== descriptor.bytes || (await sha256Hex(bytes)) !== descriptor.sha256) {
      throw new Error(`Artifact ${descriptor.id} failed its integrity check.`);
    }

    await this.#makeRoom(bytes.byteLength);
    await this.#put({
      id: descriptor.id,
      sha256: descriptor.sha256,
      rulesFingerprint: descriptor.rulesFingerprint,
      bytes,
      size: bytes.byteLength,
      lastAccessed: Date.now(),
    });
    return bytes;
  }

  async clear(): Promise<void> {
    const database = await this.#open();
    const transaction = database.transaction(STORE_NAME, "readwrite");
    transaction.objectStore(STORE_NAME).clear();
    await transactionComplete(transaction);
  }

  async #open(): Promise<IDBDatabase> {
    if (!this.#database) {
      this.#database = new Promise((resolve, reject) => {
        const request = indexedDB.open(DATABASE_NAME, DATABASE_VERSION);
        request.addEventListener("upgradeneeded", () => {
          if (!request.result.objectStoreNames.contains(STORE_NAME)) {
            request.result.createObjectStore(STORE_NAME, { keyPath: "id" });
          }
        });
        request.addEventListener("success", () => resolve(request.result), { once: true });
        request.addEventListener("error", () => reject(request.error ?? new Error("Could not open artifact cache.")), {
          once: true,
        });
      });
    }
    return this.#database;
  }

  async #get(id: string): Promise<StoredArtifact | undefined> {
    const database = await this.#open();
    const transaction = database.transaction(STORE_NAME, "readonly");
    const value = await requestResult(transaction.objectStore(STORE_NAME).get(id));
    await transactionComplete(transaction);
    return value as StoredArtifact | undefined;
  }

  async #put(record: StoredArtifact): Promise<void> {
    const database = await this.#open();
    const transaction = database.transaction(STORE_NAME, "readwrite");
    transaction.objectStore(STORE_NAME).put(record);
    await transactionComplete(transaction);
  }

  async #delete(id: string): Promise<void> {
    const database = await this.#open();
    const transaction = database.transaction(STORE_NAME, "readwrite");
    transaction.objectStore(STORE_NAME).delete(id);
    await transactionComplete(transaction);
  }

  async #records(): Promise<StoredArtifact[]> {
    const database = await this.#open();
    const transaction = database.transaction(STORE_NAME, "readonly");
    const values = await requestResult(transaction.objectStore(STORE_NAME).getAll());
    await transactionComplete(transaction);
    return values as StoredArtifact[];
  }

  async #makeRoom(incomingBytes: number): Promise<void> {
    const records = await this.#records();
    for (const id of selectEvictions(records, incomingBytes, this.capacityBytes)) await this.#delete(id);
  }
}
