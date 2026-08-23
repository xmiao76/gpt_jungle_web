import type { Difficulty, ExportGameResult, JungleSave, Side } from "./types";
import { isGameStateData } from "../protocol";

function isDifficulty(value: unknown): value is Difficulty {
  return value === "easy" || value === "medium" || value === "hard";
}

function isSide(value: unknown): value is Side {
  return value === "blue" || value === "red";
}

export function createSave(exported: ExportGameResult, boardFlipped: boolean): JungleSave {
  return {
    schema: "gpt-jungle-save",
    version: 1,
    engineVersion: exported.engineVersion,
    createdAt: new Date().toISOString(),
    gameState: exported.gameState,
    settings: {
      difficulty: exported.difficulty,
      humanSide: exported.humanSide,
      boardFlipped,
    },
  };
}

export function parseSave(text: string): JungleSave {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    throw new Error("The selected file is not valid JSON.");
  }
  if (!parsed || typeof parsed !== "object") {
    throw new Error("The selected file does not contain a Jungle save.");
  }
  const candidate = parsed as Partial<JungleSave>;
  if (candidate.schema !== "gpt-jungle-save" || candidate.version !== 1) {
    throw new Error("This save format or version is not supported.");
  }
  if (!isGameStateData(candidate.gameState)) {
    throw new Error("The save contains invalid game state.");
  }
  if (!candidate.settings || !isDifficulty(candidate.settings.difficulty) || !isSide(candidate.settings.humanSide)) {
    throw new Error("The save contains invalid game settings.");
  }
  if (typeof candidate.settings.boardFlipped !== "boolean") {
    throw new Error("The save contains an invalid board orientation.");
  }
  if (typeof candidate.engineVersion !== "string" || typeof candidate.createdAt !== "string") {
    throw new Error("The save metadata is incomplete.");
  }
  return candidate as JungleSave;
}

export function downloadSave(save: JungleSave): void {
  const blob = new Blob([`${JSON.stringify(save, null, 2)}\n`], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `jungle-${save.createdAt.replace(/[:.]/g, "-")}.json`;
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  URL.revokeObjectURL(url);
}
