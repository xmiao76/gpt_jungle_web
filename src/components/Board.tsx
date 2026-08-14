import type { CSSProperties, KeyboardEvent } from "react";
import { boardBackgroundAsset, pieceAsset, terrainAsset } from "../assets";
import { coordinateOf, displayToCanonical, canonicalToDisplay, columnOf, rowOf } from "../game/orientation";
import { terrainAt, terrainLabel } from "../game/terrain";
import { PIECE_NAMES, type GameSnapshot, type LegalTarget } from "../game/types";

interface BoardProps {
  snapshot: GameSnapshot;
  flipped: boolean;
  selected: number | null;
  targets: LegalTarget[];
  disabled: boolean;
  onSquare: (canonicalIndex: number) => void;
}

function onGridKeyDown(event: KeyboardEvent<HTMLDivElement>): void {
  const target = event.target as HTMLElement;
  const value = target.dataset.displayIndex;
  if (value === undefined) return;
  const current = Number(value);
  const row = rowOf(current);
  const column = columnOf(current);
  let next = current;
  if (event.key === "ArrowLeft" && column > 0) next -= 1;
  else if (event.key === "ArrowRight" && column < 6) next += 1;
  else if (event.key === "ArrowUp" && row > 0) next -= 7;
  else if (event.key === "ArrowDown" && row < 8) next += 7;
  else if (event.key === "Home") next = row * 7;
  else if (event.key === "End") next = row * 7 + 6;
  else return;
  event.preventDefault();
  const nextButton = event.currentTarget.querySelector<HTMLButtonElement>(`[data-display-index="${next}"]`);
  nextButton?.focus();
}

export function Board({ snapshot, flipped, selected, targets, disabled, onSquare }: BoardProps) {
  const targetMap = new Map(targets.map((target) => [target.destination, target]));
  const lastOrigin = snapshot.lastMove?.origin;
  const lastDestination = snapshot.lastMove?.destination;
  const frameStyle = { "--board-background-image": `url(${boardBackgroundAsset})` } as CSSProperties;

  return (
    <div className="board-frame" style={frameStyle}>
      <div
        className={`board${disabled ? " board--disabled" : ""}`}
        role="grid"
        aria-label={`Jungle board, ${flipped ? "flipped" : "canonical"} orientation`}
        onKeyDown={onGridKeyDown}
      >
        <div className="square-layer">
          {Array.from({ length: 63 }, (_, displayIndex) => {
            const canonicalIndex = displayToCanonical(displayIndex, flipped);
            const terrain = terrainAt(canonicalIndex);
            const piece = snapshot.gameState.board[canonicalIndex];
            const target = targetMap.get(canonicalIndex);
            const classes = [
              "board-square",
              selected === canonicalIndex ? "board-square--selected" : "",
              target ? (target.capture ? "board-square--capture" : "board-square--legal") : "",
              lastOrigin === canonicalIndex ? "board-square--last-origin" : "",
              lastDestination === canonicalIndex ? "board-square--last-destination" : "",
            ].filter(Boolean).join(" ");
            const label = [
              coordinateOf(canonicalIndex),
              terrainLabel(terrain),
              piece ? `${piece.side} ${PIECE_NAMES[piece.kind]}` : "empty",
              target ? (target.capture ? "legal capture" : "legal move") : "",
            ].filter(Boolean).join(", ");
            return (
              <button
                key={displayIndex}
                type="button"
                role="gridcell"
                className={classes}
                data-display-index={displayIndex}
                data-canonical-index={canonicalIndex}
                aria-label={label}
                disabled={disabled}
                onClick={() => onSquare(canonicalIndex)}
                style={{ backgroundImage: `url(${terrainAsset(terrain)})` }}
              >
                {terrain !== "land" ? <span className="terrain-caption">{terrainLabel(terrain)}</span> : null}
                {target ? <span className="move-marker" aria-hidden="true" /> : null}
              </button>
            );
          })}
        </div>

        <div className="piece-layer">
          {snapshot.gameState.board.map((piece, canonicalIndex) => {
            if (!piece) return null;
            const displayIndex = canonicalToDisplay(canonicalIndex, flipped);
            const style = {
              "--piece-row": rowOf(displayIndex),
              "--piece-column": columnOf(displayIndex),
            } as CSSProperties;
            return (
              <div
                className={`piece piece--${piece.side}`}
                style={style}
                key={`${piece.side}-${piece.kind}`}
                data-piece={`${piece.side}-${piece.kind.toLowerCase()}`}
              >
                <img
                  src={pieceAsset(piece)}
                  alt={`${piece.side === "blue" ? "Blue" : "Red"} ${PIECE_NAMES[piece.kind]} at ${coordinateOf(canonicalIndex)}`}
                  draggable={false}
                />
                <span>{PIECE_NAMES[piece.kind]}</span>
              </div>
            );
          })}
        </div>
      </div>
      <div className="board-orientation" aria-hidden="true">
        <span>{flipped ? "Blue view" : "Red side"}</span>
        <span>{flipped ? "Red side" : "Blue view"}</span>
      </div>
    </div>
  );
}
