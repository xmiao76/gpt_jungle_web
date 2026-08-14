import { fireEvent, render, screen } from "@testing-library/react";
import { Board } from "./Board";
import { snapshotFixture } from "../test/fixtures";

describe("Board", () => {
  it("renders 63 accessible cells and canonical piece labels", () => {
    render(
      <Board snapshot={snapshotFixture()} flipped={false} selected={null} targets={[]} disabled={false} onSquare={() => undefined} />,
    );
    expect(screen.getAllByRole("gridcell")).toHaveLength(63);
    expect(screen.getByRole("gridcell", { name: /A7, Land, blue Elephant/i })).toBeInTheDocument();
  });

  it("maps flipped display clicks back to canonical indices", () => {
    const onSquare = vi.fn();
    render(
      <Board
        snapshot={snapshotFixture()}
        flipped
        selected={42}
        targets={[{ destination: 35, capture: false, captured: null, jump: false, note: "" }]}
        disabled={false}
        onSquare={onSquare}
      />,
    );
    fireEvent.click(screen.getByRole("gridcell", { name: /A6, Land, empty, legal move/i }));
    expect(onSquare).toHaveBeenCalledWith(35);
    expect(document.querySelector('[data-display-index="27"]')).toHaveAttribute("data-canonical-index", "35");
  });

  it("keeps every cell inert while the engine is busy", () => {
    const onSquare = vi.fn();
    render(
      <Board snapshot={snapshotFixture()} flipped={false} selected={null} targets={[]} disabled onSquare={onSquare} />,
    );
    fireEvent.click(screen.getAllByRole("gridcell")[0]);
    expect(onSquare).not.toHaveBeenCalled();
  });

  it("renders distinct capture styling and useful animal alternative text", () => {
    render(
      <Board
        snapshot={snapshotFixture()}
        flipped={false}
        selected={42}
        targets={[{ destination: 14, capture: true, captured: "Rat", jump: false, note: "" }]}
        disabled={false}
        onSquare={() => undefined}
      />,
    );
    expect(screen.getByRole("gridcell", { name: /A3, Land, red Rat, legal capture/i })).toHaveClass("board-square--capture");
    expect(screen.getByAltText("Blue Elephant at A7")).toBeInTheDocument();
  });
});
