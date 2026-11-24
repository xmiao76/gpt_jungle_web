// The conformance host owns these rules independently of the Rust guest.
export const animals = [
  "rat",
  "cat",
  "dog",
  "wolf",
  "leopard",
  "tiger",
  "lion",
  "elephant",
];
const water = new Set([22, 23, 25, 26, 29, 30, 32, 33, 36, 37, 39, 40]);
const traps = { red: new Set([52, 58, 60]), blue: new Set([2, 4, 10]) };
const den = { red: 59, blue: 3 };
const axes = [
  [1, 0],
  [-1, 0],
  [0, 1],
  [0, -1],
];
export const opposite = (side) => (side === "red" ? "blue" : "red");
const sideOf = (code) => (code > 8 ? "red" : "blue");
const rank = (code) => ((code - 1) % 8) + 1;
export function opening(firstSide = "red") {
  const board = Array(63).fill(0);
  for (const [side, squares] of Object.entries({
    red: [48, 50, 54, 44, 46, 56, 62, 42],
    blue: [14, 12, 8, 18, 16, 6, 0, 20],
  })) {
    squares.forEach((sq, k) => (board[sq] = k + 1 + (side === "red" ? 8 : 0)));
  }
  return {
    rev: 0,
    board,
    firstSide,
    toPlay: firstSide,
    history: [],
    noCapturePlies: 0,
    status: "playing",
    winner: null,
    reason: "",
  };
}
function capture(board, src, dst) {
  const a = board[src],
    b = board[dst];
  if (!b) return true;
  if (sideOf(a) === sideOf(b) || water.has(src) !== water.has(dst))
    return false;
  if (traps[sideOf(a)].has(dst)) return true;
  if (traps[sideOf(b)].has(src)) return false;
  if (rank(a) === 1 && rank(b) === 8) return true;
  if (rank(a) === 8 && rank(b) === 1) return false;
  return rank(a) >= rank(b);
}
function step(square, dr, dc) {
  const r = Math.floor(square / 7) + dr,
    c = (square % 7) + dc;
  return r >= 0 && r < 9 && c >= 0 && c < 7 ? r * 7 + c : -1;
}
export function legal(state) {
  if (state.status !== "playing") return [];
  const moves = [];
  for (const [src, piece] of state.board.entries()) {
    if (!piece || sideOf(piece) !== state.toPlay) continue;
    for (const [dr, dc] of axes) {
      let dst = step(src, dr, dc);
      if (dst < 0) continue;
      if (water.has(dst) && rank(piece) !== 1) {
        if (rank(piece) !== 6 && rank(piece) !== 7) continue;
        let n = 0,
          blocked = false;
        while (water.has(dst)) {
          blocked ||= Boolean(state.board[dst]);
          n++;
          dst = step(dst, dr, dc);
        }
        if (blocked || dst < 0 || (n !== 2 && !(n === 3 && rank(piece) === 7)))
          continue;
      }
      if (dst !== den[state.toPlay] && capture(state.board, src, dst))
        moves.push({ src, dst });
    }
  }
  return moves.sort((a, b) => a.src - b.src || a.dst - b.dst);
}
const key = (s) => s.toPlay + s.board.join(",");
export class Referee {
  constructor(start = opening()) {
    this.state = structuredClone(start);
    this.counts = new Map([[key(start), 1]]);
  }
  apply(m) {
    const old = this.state;
    if (!legal(old).some((p) => p.src === m.src && p.dst === m.dst))
      throw Error("Illegal referee move");
    const board = old.board.slice(),
      captured = board[m.dst];
    board[m.dst] = board[m.src];
    board[m.src] = 0;
    const s = {
      ...old,
      rev: old.rev + 1,
      board,
      toPlay: opposite(old.toPlay),
      history: [...old.history, { src: m.src, dst: m.dst }],
      noCapturePlies: captured ? 0 : old.noCapturePlies + 1,
    };
    const k = key(s),
      count = (this.counts.get(k) ?? 0) + 1;
    this.counts.set(k, count);
    if (m.dst === den[s.toPlay]) s.reason = "den";
    else if (!board.some((p) => p && sideOf(p) === s.toPlay))
      s.reason = "elimination";
    else if (!legal(s).length) s.reason = "immobilization";
    else if (count === 3) s.reason = "repetition";
    else if (s.noCapturePlies >= 100) s.reason = "no-capture";
    if (s.reason) {
      s.status = ["repetition", "no-capture"].includes(s.reason)
        ? "drawn"
        : "won";
      s.winner = s.status === "won" ? old.toPlay : null;
    }
    return (this.state = s);
  }
  forfeit(reason) {
    if (
      this.state.status !== "playing" ||
      !["timeout", "trap", "invalid-move"].includes(reason)
    )
      throw Error("Invalid forfeit");
    return (this.state = {
      ...this.state,
      status: "forfeited",
      rev: this.state.rev + 1,
      winner: opposite(this.state.toPlay),
      reason,
    });
  }
}
export function wire(state) {
  return {
    ...state,
    winner: state.winner ?? undefined,
    board: state.board.map((p) =>
      p ? { owner: sideOf(p), kind: animals[rank(p) - 1] } : undefined,
    ),
  };
}
