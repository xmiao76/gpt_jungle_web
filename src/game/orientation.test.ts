import { BOARD_SIZE, canonicalToDisplay, coordinateOf, displayToCanonical } from "./orientation";

describe("board orientation", () => {
  it("round trips every canonical square in both orientations", () => {
    for (const flipped of [false, true]) {
      for (let index = 0; index < BOARD_SIZE; index += 1) {
        expect(displayToCanonical(canonicalToDisplay(index, flipped), flipped)).toBe(index);
      }
    }
  });

  it("reverses both axes without changing canonical coordinates", () => {
    expect(canonicalToDisplay(0, true)).toBe(62);
    expect(canonicalToDisplay(62, true)).toBe(0);
    expect(canonicalToDisplay(10, false)).toBe(10);
    expect(coordinateOf(0)).toBe("A1");
    expect(coordinateOf(62)).toBe("G9");
  });

  it("rejects invalid board indices", () => {
    expect(() => canonicalToDisplay(-1, false)).toThrow(RangeError);
    expect(() => canonicalToDisplay(63, true)).toThrow(RangeError);
  });
});

