import { useState } from "react";
import { Modal } from "./Modal";
import { ReleaseInfo } from "./ReleaseInfo";
import type { Options } from "../game/match";
import type { Difficulty, Side } from "../game/types";
export const defaults: Options = {
  difficulty: "medium",
  auto: false,
  firstSide: "red",
  humanSide: "red",
};
export function Setup({
  initial,
  start,
  close,
  help,
}: {
  initial: Options;
  start: (o: Options) => void;
  close?: () => void;
  help: () => void;
}) {
  const [level, setLevel] = useState(initial.difficulty),
    [auto, setAuto] = useState(initial.auto),
    [first, setFirst] = useState(initial.firstSide);
  const [humanFirst, setHumanFirst] = useState(
    initial.firstSide === initial.humanSide,
  );
  return (
    <Modal title="Start a new expedition" onClose={close}>
      <form
        className="new-game-form"
        onSubmit={(e) => {
          e.preventDefault();
          start({
            difficulty: level,
            auto,
            firstSide: first,
            humanSide: humanFirst ? first : first === "red" ? "blue" : "red",
          });
        }}
      >
        <div className="setup-row">
          <label>
            Game mode
            <select
              value={auto ? "auto" : "human"}
              onChange={(e) => setAuto(e.target.value === "auto")}
            >
              <option value="human">Human vs AI</option>
              <option value="auto">AI vs AI</option>
            </select>
          </label>
          <label>
            Difficulty
            <select
              value={level}
              onChange={(e) => setLevel(e.target.value as Difficulty)}
            >
              {["easy", "medium", "hard"].map((d) => (
                <option key={d} value={d}>
                  {d[0].toUpperCase() + d.slice(1)}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div className="setup-row">
          <label>
            First color
            <select
              value={first}
              onChange={(e) => setFirst(e.target.value as Side)}
            >
              <option value="red">Red</option>
              <option value="blue">Blue</option>
            </select>
          </label>
          {!auto && (
            <label>
              First mover
              <select
                value={humanFirst ? "human" : "ai"}
                onChange={(e) => setHumanFirst(e.target.value === "human")}
              >
                <option value="human">Human first</option>
                <option value="ai">AI first</option>
              </select>
            </label>
          )}
        </div>
        <div className="modal-actions">
          <button type="submit" className="primary-button">
            Begin game
          </button>
          <button type="button" className="secondary-button" onClick={help}>
            How to play
          </button>
        </div>
      </form>
      <ReleaseInfo />
    </Modal>
  );
}
