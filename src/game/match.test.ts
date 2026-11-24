/// <reference types="node" />
// @vitest-environment node
import { readFileSync } from "node:fs";
import { initSync } from "../../generated/standalone";
import { apply, fresh, legal, readSave } from "./match";
beforeAll(() =>
  initSync({ module: readFileSync("generated/standalone_bg.wasm") }),
);
const settings = {
  firstSide: "red",
  humanSide: "red",
  difficulty: "hard",
  auto: false,
};
const cycle = [
  { src: 42, dst: 35 },
  { src: 0, dst: 1 },
  { src: 35, dst: 42 },
  { src: 1, dst: 0 },
];
const save = (history: unknown[], extra = {}) =>
  JSON.stringify({
    schema: "gpt-jungle-save",
    version: 2,
    rulesId: "desktop-jungle-v2",
    settings,
    flipped: true,
    history,
    ...extra,
  });
it("replays repetition context and exposes the previous playable revision for undo", () => {
  const { line, flipped } = readSave(save([...cycle, ...cycle]));
  expect(flipped).toBe(true);
  expect(line[8]).toMatchObject({
    rev: 8,
    noCapturePlies: 8,
    toPlay: "red",
    status: "drawn",
    reason: "repetition",
  });
  expect(legal(line[8])).toEqual([]);
  expect(line[7].status).toBe("playing");
  expect(apply(line[7], cycle[3])).toEqual(line[8]);
  expect(() => readSave(save([...cycle, ...cycle, cycle[0]]))).toThrow();
});
it("rejects old saves, incorrect rules and invalid coordinates before replacement", () => {
  expect(() => readSave(save([], { version: 1 }))).toThrow(/earlier/);
  expect(() => readSave(save([], { rulesId: "old" }))).toThrow();
  for (const src of [-1, 63, 256, 1.2])
    expect(() => readSave(save([{ src, dst: 0 }]))).toThrow();
});
it("Blue first changes only the selected turn and preserves the opening board", () => {
  const red = fresh("red"),
    blue = fresh("blue");
  expect(blue.board).toEqual(red.board);
  expect(legal(blue)).toHaveLength(24);
  const loaded = readSave(
    save([{ src: 0, dst: 1 }], {
      settings: { ...settings, firstSide: "blue" },
    }),
  );
  expect(loaded.line[1].toPlay).toBe("red");
});
