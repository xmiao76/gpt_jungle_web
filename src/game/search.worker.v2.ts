import { initSync, search } from "../../generated/standalone";
self.onmessage = (event: MessageEvent) => {
  const q = event.data;
  try {
    if (q.engine) {
      initSync({ module: q.engine });
      self.postMessage({ ready: true });
      return;
    }
    const choice = JSON.parse(
      search(
        q.json,
        q.level,
        q.seed,
        0,
        q.deadline - performance.timeOrigin,
        (json: string) =>
          self.postMessage({ id: q.id, progress: JSON.parse(json) }),
      ),
    );
    self.postMessage({ id: q.id, choice });
  } catch (error) {
    self.postMessage({ id: q.id, error: String(error) });
  }
};
