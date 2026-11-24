import assert from "node:assert/strict";
import { readFileSync, mkdirSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { performance } from "node:perf_hooks";
import { compile } from "../conformance/compile.mjs";
import { instantiate } from "../conformance/seat.mjs";
import { checkInfo } from "../conformance/host.mjs";
import {
  Referee,
  opening,
  legal,
  wire,
  animals,
} from "../conformance/referee.mjs";
import {
  initSync,
  apply_move,
  legal_moves,
  search,
  opening as rustOpening,
} from "../generated/standalone.js";
const read = (p) => new Uint8Array(readFileSync(p)),
  json = (p) => JSON.parse(readFileSync(p));
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const meta = json("generated/metadata.json"),
  release = read("release/" + meta.filename);
const ref = read("dist/conformance/contract.wasm"),
  times = {};
let before = performance.now();
const c = await compile(release, ref);
times.transpileMs = performance.now() - before;
assert(times.transpileMs < 30000);
assert.equal(c.hash, meta.sha256);
const fixture = await compile(read("generated/first-legal.wasm"), ref);
const loadSource = (source) =>
  import(
    "data:text/javascript;base64," + Buffer.from(source).toString("base64")
  );
const seat = (component) => instantiate(component, loadSource);
initSync({ module: read("generated/standalone_bg.wasm") });
const rustMoves = (s) => JSON.parse(legal_moves(JSON.stringify(s)));
const rustApply = (s, p) =>
  JSON.parse(apply_move(JSON.stringify(s), p.src, p.dst));
const verify = (s) => assert.deepEqual(rustMoves(s), legal(s));
for (const first of ["red", "blue"]) {
  const s = opening(first);
  assert.deepEqual(JSON.parse(rustOpening(first === "red")), s);
  verify(s);
  assert.equal(legal(s).length, 24);
  assert.deepEqual(
    legal(s)[0],
    first === "red" ? { src: 42, dst: 35 } : { src: 0, dst: 1 },
  );
}
assert.deepEqual(legal(opening()).at(-1), { src: 62, dst: 61 });
const sparse = (placements, quiet = 0) => {
  const s = opening();
  s.board.fill(0);
  s.noCapturePlies = quiet;
  for (const [sq, value] of Object.entries(placements)) {
    const [side, kind] = value.split(":");
    s.board[Number(sq)] = animals.indexOf(kind) + 1 + (side === "red" ? 8 : 0);
  }
  return s;
};
for (const [name, board, [src, dst], expected] of json(
  "rules/legal-cases.json",
)) {
  const s = sparse(board);
  verify(s);
  assert.equal(
    rustMoves(s).some((m) => m.src === src && m.dst === dst),
    expected,
    name,
  );
}
for (const [board, src, dst, quiet, after, reason] of [
  [{ 2: "red:cat", 0: "blue:rat" }, 2, 3, 99, 100, "den"],
  [{ 42: "red:rat", 43: "blue:elephant" }, 42, 43, 99, 0, "elimination"],
  [
    { 2: "red:dog", 7: "red:cat", 0: "blue:rat" },
    2,
    1,
    99,
    100,
    "immobilization",
  ],
  [{ 42: "red:cat", 6: "blue:dog" }, 42, 43, 98, 99, ""],
  [{ 42: "red:cat", 6: "blue:dog" }, 42, 43, 99, 100, "no-capture"],
  [{ 42: "red:wolf", 43: "blue:cat", 6: "blue:dog" }, 42, 43, 99, 0, ""],
]) {
  const s = sparse(board, quiet),
    r = new Referee(s),
    next = rustApply(s, { src, dst });
  assert.deepEqual(next, r.apply({ src, dst }));
  assert.equal(next.noCapturePlies, after);
  assert.equal(next.reason, reason);
}
for (const quiet of [0, 92]) {
  const r = new Referee(sparse({ 42: "red:cat", 6: "blue:dog" }, quiet));
  let s = r.state;
  for (const [src, dst] of [
    [42, 43],
    [6, 5],
    [43, 42],
    [5, 6],
    [42, 43],
    [6, 5],
    [43, 42],
    [5, 6],
  ]) {
    s = rustApply(s, { src, dst });
    assert.deepEqual(s, r.apply({ src, dst }));
    assert.equal(s.reason, s.rev === 8 ? "repetition" : "");
  }
  assert.equal(s.toPlay, "red");
}
const failRef = new Referee(),
  beforeForfeit = failRef.state;
assert.deepEqual(failRef.forfeit("timeout"), {
  ...beforeForfeit,
  rev: 1,
  status: "forfeited",
  winner: "blue",
  reason: "timeout",
});
before = performance.now();
const a = await seat(c),
  b = await seat(c);
const info = a.info();
checkInfo(info);
for (const key of Object.keys(info)) assert.equal(info[key], meta[key]);
for (const s of [a, b]) s.init({ seat: "red", difficulty: "medium", seed: 1 });
const start = opening(),
  w = wire(start),
  moves = legal(start);
const pick = a.move(w, moves, 1000);
assert(moves[pick]);
assert.equal(b.move(w, moves, 1000), pick);
const f = a.frame(w, false, 1000),
  flipped = a.frame(w, true, 1000);
const opponent = await seat(fixture);
opponent.init({ seat: "blue", difficulty: "medium", seed: 1 });
assert.equal(opponent.move(w, moves, 20000), 0);
assert.notEqual(sha(opponent.frame(w, false, 1000)), sha(f));
assert.notEqual(sha(f), sha(flipped));
assert.equal(sha(f), sha(a.frame(w, false, 1000)));
for (let i = 3; i < f.length; i += 4) assert.equal(f[i], 255);
assert.equal(a.move(w, moves, 1000), pick);
times.smokeMs = performance.now() - before;
assert(times.smokeMs < 20000);
for (const s of [
  start,
  rustApply(start, moves[0]),
  sparse({ 21: "red:tiger", 24: "blue:dog", 0: "blue:rat" }),
]) {
  for (const budget of [1, 1000, 20000]) {
    const hostMoves = legal(s),
      web = JSON.parse(search(JSON.stringify(s), "medium", 1, budget, 0));
    assert.equal(a.move(wire(s), hostMoves, budget), web.index);
    assert(web.nodes <= budget);
  }
}
const jump = sparse({ 21: "red:tiger", 24: "blue:dog", 0: "blue:rat" }),
  jumped = rustApply(jump, { src: 21, dst: 24 });
assert.notEqual(
  sha(a.frame(wire(jumped), false, 250)),
  sha(a.frame(wire(jumped), false, 1000)),
);
// A replay can seek straight to the settled capture in a new seat.
const replay = await seat(c);
replay.init({ seat: "blue", difficulty: "medium", seed: 1 });
for (const flip of [false, true])
  assert.equal(sha(replay.frame(wire(jumped), flip, 1000)),
    sha(a.frame(wire(jumped), flip, 1000)));
assert.notEqual(sha(a.frame(wire(failRef.state), false, 1000)), sha(f));
const hard = await seat(c);
hard.init({ seat: "red", difficulty: "hard", seed: 1 });
for (const budget of [1000, 5000, 20000, 100000, 500000]) {
  const t = performance.now();
  assert(moves[hard.move(w, moves, budget)]);
  times["nodes" + budget] = performance.now() - t;
}
assert(times.nodes20000 < 1000);
assert(times.nodes500000 < 5000);
mkdirSync("docs/evidence", { recursive: true });
writeFileSync(
  "docs/evidence/admission.json",
  JSON.stringify(
    {
      hash: c.hash,
      bytes: release.length,
      info,
      times,
      portableCases: json("rules/legal-cases.json").length,
      frameHashes: [sha(f), sha(flipped)],
      fixtureHash: fixture.hash,
    },
    null,
    2,
  ) + "\n",
);
console.log(
  "Release admission, canonical fixtures, deterministic search, render and cross-build parity passed.",
  times,
);
