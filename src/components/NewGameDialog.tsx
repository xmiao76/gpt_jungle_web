import { useState, type FormEvent } from "react";
import type { Difficulty, Side } from "../game/types";
import { Modal } from "./Modal";

interface NewGameDialogProps {
  currentDifficulty: Difficulty;
  canClose: boolean;
  onClose: () => void;
  onStart: (difficulty: Difficulty, humanSide: Side) => Promise<void>;
}

export function NewGameDialog({ currentDifficulty, canClose, onClose, onStart }: NewGameDialogProps) {
  const [difficulty, setDifficulty] = useState<Difficulty>(currentDifficulty);
  const [starter, setStarter] = useState<"human" | "ai">("human");
  const [submitting, setSubmitting] = useState(false);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setSubmitting(true);
    try {
      await onStart(difficulty, starter === "human" ? "blue" : "red");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal title="Start a new expedition" onClose={canClose ? onClose : undefined}>
      <form className="new-game-form" onSubmit={(event) => void submit(event)}>
        <fieldset>
          <legend>Choose AI difficulty</legend>
          <div className="choice-grid choice-grid--three">
            {(["easy", "medium", "hard"] as const).map((choice) => (
              <label className={`choice-card${difficulty === choice ? " choice-card--selected" : ""}`} key={choice}>
                <input
                  type="radio"
                  name="difficulty"
                  value={choice}
                  checked={difficulty === choice}
                  onChange={() => setDifficulty(choice)}
                />
                <strong>{choice[0].toUpperCase() + choice.slice(1)}</strong>
                <small>{choice === "easy" ? "Quick explorer" : choice === "medium" ? "Balanced rival" : "Deep tactician"}</small>
              </label>
            ))}
          </div>
        </fieldset>
        <fieldset>
          <legend>Who takes Blue's first move?</legend>
          <div className="choice-grid">
            <label className={`choice-card${starter === "human" ? " choice-card--selected" : ""}`}>
              <input type="radio" name="starter" checked={starter === "human"} onChange={() => setStarter("human")} />
              <strong>Human starts</strong>
              <small>You control Blue</small>
            </label>
            <label className={`choice-card${starter === "ai" ? " choice-card--selected" : ""}`}>
              <input type="radio" name="starter" checked={starter === "ai"} onChange={() => setStarter("ai")} />
              <strong>AI starts</strong>
              <small>You control Red</small>
            </label>
          </div>
        </fieldset>
        <button className="primary-button primary-button--large" type="submit" disabled={submitting}>
          {submitting ? "Preparing board..." : "Begin game"}
        </button>
      </form>
    </Modal>
  );
}

