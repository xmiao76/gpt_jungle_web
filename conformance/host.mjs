import { Referee, legal, opening, wire } from "./referee.mjs";
const getBytes = async (url) => {
  const r = await fetch(url);
  if (!r.ok) throw Error(r.status + " " + url);
  return new Uint8Array(await r.arrayBuffer());
};
export async function admission(bytes) {
  const ref = await getBytes("./contract.wasm"),
    worker = new Worker("./compile.worker.mjs", { type: "module" });
  try {
    return await new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => reject(Error("Transpilation timed out")),
        30000,
      );
      worker.onmessage = (e) => {
        clearTimeout(timer);
        if (e.data.error) reject(Error(e.data.error));
        else resolve(e.data.value);
      };
      worker.onerror = (e) => {
        clearTimeout(timer);
        reject(Error(e.message));
      };
      worker.postMessage({ bytes, reference: ref });
    });
  } finally {
    worker.terminate();
  }
}
export class Seat {
  constructor() {
    this.worker = new Worker("./seat.worker.mjs", { type: "module" });
    this.id = 0;
  }
  call(method, ...args) {
    const id = ++this.id,
      worker = this.worker;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => {
          worker.terminate();
          reject(Error(method + " timeout"));
        },
        method === "frame" ? 2000 : method === "move" ? 5000 : 20000,
      );
      worker.onerror = (e) => {
        clearTimeout(timer);
        reject(Error(e.message));
      };
      worker.onmessage = (e) => {
        if (e.data.id !== id) return;
        clearTimeout(timer);
        if (e.data.error) reject(Error(e.data.error));
        else resolve(e.data.value);
      };
      worker.postMessage({ id, method, args });
    });
  }
  close() {
    this.worker.terminate();
  }
}
export function checkInfo(info) {
  if (
    info.rulesId !== "desktop-jungle-v2" ||
    info.rulesDigest !==
      "a457bbc009e975a9eafe5ff1f8fa40ca2548f2a689b121c8c4ed99dd0f3fdc57"
  )
    throw Error("Rules identity mismatch");
  for (const k of [
    "title",
    "gameVersion",
    "codeAgent",
    "model",
    "modelPlatform",
    "engineVersion",
    "runtime",
    "githubUrl",
    "rulesId",
    "rulesDigest",
  ]) {
    if (
      typeof info[k] !== "string" ||
      info[k].length > 160 ||
      (!["modelPlatform", "engineVersion"].includes(k) && !info[k].trim())
    )
      throw Error("Invalid metadata " + k);
  }
  if (!info.githubUrl.startsWith("https://github.com/"))
    throw Error("Missing public source URL");
}
export async function smoke(component) {
  const seat = new Seat();
  try {
    const info = await seat.call("open", component);
    checkInfo(info);
    await seat.call("init", { seat: "red", difficulty: "medium", seed: 1 });
    const s = opening(),
      moves = legal(s);
    const idx = await seat.call("move", wire(s), moves, 1000);
    if (!Number.isInteger(idx) || !moves[idx])
      throw Error("Illegal opening index");
    for (const flip of [false, true])
      await seat.call("frame", wire(s), flip, 1000);
    return info;
  } finally {
    seat.close();
  }
}
export async function match(red, blue, onFrame = () => {}) {
  const seats = [new Seat(), new Seat()],
    ref = new Referee(),
    replay = [];
  const components = [red, blue];
  const openSeat = async (i) => {
    const seat = seats[i];
    checkInfo(await seat.call("open", components[i]));
    await seat.call("init", {
      seat: i ? "blue" : "red",
      difficulty: "medium",
      seed: 20261120,
    });
  };
  try {
    await Promise.all(seats.map((_, i) => openSeat(i)));
    while (true) {
      const s = ref.state;
      replay.push(s);
      const w = wire(s);
      const frames = await Promise.all(
        seats.map((seat, i) => seat.call("frame", w, Boolean(i), 1000)),
      );
      onFrame(s, frames);
      if (s.status !== "playing") break;
      const moves = legal(s),
        i = s.toPlay === "blue" ? 1 : 0;
      let index;
      try {
        index = await seats[i].call("move", w, moves, 20000);
      } catch (error) {
        ref.forfeit(String(error).includes("timeout") ? "timeout" : "trap");
        seats[i].close();
        seats[i] = new Seat();
        await openSeat(i);
        continue;
      }
      if (!Number.isInteger(index) || !moves[index])
        ref.forfeit("invalid-move");
      else ref.apply(moves[index]);
    }
    return replay;
  } finally {
    seats.forEach((s) => s.close());
  }
}
export async function loadDefault() {
  const meta = await (await fetch("./metadata.json")).json();
  return {
    meta,
    file: await getBytes("../" + meta.filename),
    fixture: await getBytes("./first-legal.wasm"),
  };
}
