export const BOARD_COLS = 7;
export const BOARD_ROWS = 9;
export const BOARD_SIZE = BOARD_COLS * BOARD_ROWS;

export function assertBoardIndex(index: number): void {
  if (!Number.isInteger(index) || index < 0 || index >= BOARD_SIZE) {
    throw new RangeError(`Board index must be an integer from 0 to ${BOARD_SIZE - 1}.`);
  }
}

export function canonicalToDisplay(index: number, flipped: boolean): number {
  assertBoardIndex(index);
  return flipped ? BOARD_SIZE - 1 - index : index;
}

export function displayToCanonical(index: number, flipped: boolean): number {
  return canonicalToDisplay(index, flipped);
}

export function rowOf(index: number): number {
  assertBoardIndex(index);
  return Math.floor(index / BOARD_COLS);
}

export function columnOf(index: number): number {
  assertBoardIndex(index);
  return index % BOARD_COLS;
}

export function coordinateOf(index: number): string {
  return `${String.fromCharCode(65 + columnOf(index))}${rowOf(index) + 1}`;
}

