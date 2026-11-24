import { initSync, search } from "../../generated/standalone";
import { engine } from "../../generated/embedded";
import WorkerConstructor from "./search.worker.v2?worker&inline";
import { apply, legal, type Match, type Ply } from "./match";
import type { Difficulty } from "./types";
export const decode = (s: string) =>
  Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
export interface Choice {
  index: number;
  nodes: number;
  depth: number;
  proven: boolean;
  score: number;
}
const pause = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
export class Runtime {
  private worker: Worker | null = null;
  private token = 0;
  private abandon: (() => void) | null = null;
  readonly bytes = decode(engine);
  mode = "main thread";
  async start(onCompiled: () => void = () => {}) {
    initSync({ module: this.bytes });
    onCompiled();
    await this.warm();
  }
  private async warm() {
    let worker: Worker;
    try {
      worker = new WorkerConstructor();
    } catch {
      return;
    }
    this.worker = worker;
    const ready = await new Promise<boolean>((resolve) => {
      const timer = setTimeout(() => resolve(false), 4000);
      const done = (value: boolean) => {
        clearTimeout(timer);
        resolve(value);
      };
      worker.onmessage = (e) => done(e.data.ready === true);
      worker.onerror = (e) => {
        e.preventDefault();
        done(false);
      };
      try {
        worker.postMessage({ engine: this.bytes });
      } catch {
        done(false);
      }
    });
    if (this.worker !== worker) return;
    if (ready) this.mode = "worker";
    else {
      worker.terminate();
      this.worker = null;
      this.mode = "main thread";
    }
  }
  cancel() {
    this.token++;
    this.abandon?.();
    this.abandon = null;
  }
  close() {
    this.cancel();
    this.worker?.terminate();
    this.worker = null;
  }
  async move(
    state: Match,
    level: Difficulty,
    seed = 1,
  ): Promise<{
    move: Ply;
    choice: Choice;
    elapsed: number;
    started: number;
  } | null> {
    this.cancel();
    const id = this.token;
    const start = performance.now(),
      duration = { easy: 250, medium: 750, hard: 1000 }[level];
    const end = start + duration - (level === "hard" ? 40 : 18),
      moves = legal(state);
    if (!moves.length) return null;
    const fallback: Choice = {
      index: 0,
      nodes: 0,
      depth: 0,
      score: 0,
      proven: moves.length === 1,
    };
    let choice = fallback;
    if (!choice.proven && this.worker && this.mode === "worker") {
      const worker = this.worker;
      choice = await new Promise<Choice>((resolve) => {
        let settled = false,
          latest = fallback;
        const finish = (v: Choice) => {
          if (!settled) {
            settled = true;
            clearTimeout(timer);
            this.abandon = null;
            resolve(v);
          }
        };
        const timer = setTimeout(
          () => finish(latest),
          Math.max(0, end - performance.now() + 8),
        );
        this.abandon = () => {
          finish(fallback);
          worker.terminate();
          this.worker = null;
          this.mode = "main thread";
          void this.warm();
        };
        worker.onmessage = (e) => {
          if (e.data.id !== id) return;
          if (e.data.progress) latest = e.data.progress;
          else finish(e.data.choice ?? latest);
        };
        worker.postMessage({
          id,
          json: JSON.stringify(state),
          level,
          seed,
          deadline: performance.timeOrigin + end,
        });
      });
    } else if (!choice.proven) {
      // Round-robin root moves in short time slices. Each completed round replaces
      // the previous decision, leaving the event loop free between child searches.
      let round = 0;
      while (this.token === id && performance.now() < end) {
        let best = -Infinity,
          index = 0,
          nodes = 0,
          depth = 128,
          complete = true;
        for (const [i, m] of moves.entries()) {
          if (this.token !== id || performance.now() >= end) {
            complete = false;
            break;
          }
          const child = apply(state, m);
          if (child.winner === state.toPlay) {
            choice = { ...fallback, index: i, proven: true };
            break;
          }
          const r: Choice =
            child.status === "drawn"
              ? { ...fallback, score: 0 }
              : JSON.parse(
                  search(
                    JSON.stringify(child),
                    level,
                    seed,
                    0,
                    Math.min(
                      end,
                      performance.now() + Math.min(12, 3 + round * 2),
                    ),
                  ),
                );
          if (-r.score > best) {
            best = -r.score;
            index = i;
          }
          nodes += r.nodes;
          depth = Math.min(depth, r.depth);
          await pause();
        }
        if (choice.proven) break;
        if (complete)
          choice = {
            index,
            score: best,
            nodes,
            depth: depth + 1,
            proven: false,
          };
        round++;
      }
    }
    if (this.token !== id) return null;
    return {
      move: moves[choice.index] ?? moves[0],
      choice,
      elapsed: performance.now() - start,
      started: start,
    };
  }
}
