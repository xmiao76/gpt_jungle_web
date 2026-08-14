import { expect, test, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import type { Difficulty, JungleSave, PieceData, Side } from "../../src/game/types";

async function waitForReady(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Entering the jungle" })).toBeVisible({ timeout: 5_000 });
  await expect(page.getByRole("dialog", { name: "Start a new expedition" })).toBeVisible({ timeout: 90_000 });
}

async function startGame(page: Page, difficulty: Difficulty = "easy", starter: "human" | "ai" = "human") {
  await waitForReady(page);
  await page.getByText(difficulty[0].toUpperCase() + difficulty.slice(1), { exact: true }).click();
  await page.getByText(starter === "human" ? "Human starts" : "AI starts", { exact: true }).click();
  await page.getByRole("button", { name: "Begin game" }).click();
  await expect(page.getByRole("dialog", { name: "Start a new expedition" })).toBeHidden();
}

async function sampleFrameRate(page: Page, durationMs = 600): Promise<number> {
  return page.evaluate((duration) => new Promise<number>((resolve) => {
    const started = performance.now();
    let frames = 0;
    const countFrame = (now: number) => {
      frames += 1;
      const elapsed = now - started;
      if (elapsed >= duration) resolve(frames * 1_000 / elapsed);
      else requestAnimationFrame(countFrame);
    };
    requestAnimationFrame(countFrame);
  }), durationMs);
}

function positionSave(
  pieces: Array<[number, PieceData]>,
  difficulty: Difficulty = "hard",
  humanSide: Side = "blue",
): JungleSave {
  const board: Array<PieceData | null> = Array.from({ length: 63 }, () => null);
  for (const [index, piece] of pieces) board[index] = piece;
  return {
    schema: "gpt-jungle-save",
    version: 1,
    engineVersion: "0.1.0",
    createdAt: "2026-08-13T00:00:00.000Z",
    gameState: {
      board,
      side_to_move: "blue",
      move_history: [],
      winner: null,
      result: "ongoing",
      result_reason: "",
    },
    settings: { difficulty, humanSide, boardFlipped: false },
  };
}

test("loads the real Pyodide engine and completes a human/AI turn", async ({ page }) => {
  await startGame(page, "easy", "human");
  await expect(page.getByRole("gridcell")).toHaveCount(63);
  await expect(page.getByText(/Python 3\.14/).first()).toBeVisible();
  expect(await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.gameState.board.filter(Boolean).length)).toBe(16);

  await page.locator('[data-canonical-index="42"]').click();
  await expect(page.locator('[data-canonical-index="35"] .move-marker')).toBeVisible();
  await page.locator('[data-canonical-index="35"]').click();
  await expect(page.locator(".move-history li")).toHaveCount(2, { timeout: 30_000 });

  const snapshot = await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot());
  expect(snapshot?.gameState.side_to_move).toBe("blue");
  expect(snapshot?.history).toHaveLength(2);
});

test("an AI move is legal for the exact revision it searched", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "The exhaustive legal-move comparison runs once in Chromium.");
  await startGame(page, "easy", "human");
  const result = await page.evaluate(async () => window.__JUNGLE_TEST__?.runVerifiedAiMove(100));
  const applied = result?.snapshot.appliedMove;
  expect(result?.snapshot.revision).toBe((result?.beforeRevision ?? -1) + 1);
  expect(result?.legalMoves).toContainEqual({ origin: applied?.origin, destination: applied?.destination });
});

test("board flip changes display only", async ({ page }) => {
  await startGame(page, "easy", "human");
  const before = await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.gameState);
  await page.getByRole("button", { name: "Flip Board" }).click();
  const after = await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.gameState);
  expect(after).toEqual(before);
  await expect(page.locator('[data-display-index="20"]')).toHaveAttribute("data-canonical-index", "42");
});

test("AI-first assigns Red to the human and moves without blocking", async ({ page }) => {
  await startGame(page, "easy", "ai");
  await expect.poll(
    async () => (await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.history.length)) ?? 0,
    { timeout: 30_000 },
  ).toBe(1);
  const snapshot = await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot());
  expect(snapshot?.humanSide).toBe("red");
  expect(snapshot?.gameState.side_to_move).toBe("red");
});

test("undo, redo, save, and load preserve the exact completed turn", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "Persistence and paired-history behavior are covered once in Chromium.");
  await startGame(page, "easy", "human");
  await page.locator('[data-canonical-index="42"]').click();
  await page.locator('[data-canonical-index="35"]').click();
  await expect.poll(
    async () => (await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.history.length)) ?? 0,
    { timeout: 30_000 },
  ).toBe(2);

  const completedTurn = await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.gameState);
  const downloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  const download = await downloadPromise;
  const savePath = await download.path();
  expect(savePath).not.toBeNull();
  const saved = JSON.parse(await readFile(savePath!, "utf8")) as JungleSave;
  expect(saved.schema).toBe("gpt-jungle-save");
  expect(saved.version).toBe(1);
  expect(saved.gameState).toEqual(completedTurn);

  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect.poll(
    async () => (await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.history.length)) ?? -1,
  ).toBe(0);
  await page.getByRole("button", { name: "Redo", exact: true }).click();
  await expect.poll(
    async () => (await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.history.length)) ?? 0,
  ).toBe(2);
  expect(await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.gameState)).toEqual(completedTurn);

  await page.getByRole("button", { name: "New Game", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "Start a new expedition" })).toBeVisible();
  await page.getByRole("button", { name: "Begin game" }).click();
  await expect.poll(
    async () => (await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.history.length)) ?? -1,
  ).toBe(0);
  const reoriented = { ...saved, settings: { ...saved.settings, boardFlipped: true } };
  await page.locator('input[type="file"]').setInputFiles({
    name: "jungle-round-trip.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify(reoriented)),
  });
  await expect(page.getByRole("status")).toContainText("Saved game loaded");
  expect(await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.gameState)).toEqual(completedTurn);
  await expect(page.locator('[data-display-index="20"]')).toHaveAttribute("data-canonical-index", "42");
});

test("Hard AI stays within its response budget while the main thread remains interactive", async ({ page, browser, browserName }) => {
  test.skip(browserName !== "chromium", "The timing acceptance test runs once in Chromium.");
  await waitForReady(page);
  const idleFramesPerSecond = await sampleFrameRate(page);
  await page.getByText("Hard", { exact: true }).click();
  await page.getByText("AI starts", { exact: true }).click();
  await page.getByRole("button", { name: "Begin game" }).click();
  await expect(page.getByText(/AI is thinking/)).toBeVisible();
  const searchFramesPerSecond = await sampleFrameRate(page);
  const interactionStarted = Date.now();
  await page.getByRole("button", { name: "How to Play" }).click();
  await expect(page.getByRole("dialog", { name: "How to play" })).toBeVisible();
  const interactionMs = Date.now() - interactionStarted;
  await page.getByRole("button", { name: "Ready to play" }).click();
  await expect.poll(
    async () => (await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.history.length)) ?? 0,
    { timeout: 30_000 },
  ).toBe(1);
  const stats = await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.aiStats);
  console.info(
    `[hard-ai-benchmark] Chromium ${browser.version()} idle-fps=${idleFramesPerSecond.toFixed(1)} ` +
    `search-fps=${searchFramesPerSecond.toFixed(1)} interaction-ms=${interactionMs} ${JSON.stringify(stats)}`,
  );
  expect(stats?.difficulty).toBe("hard");
  expect(stats?.elapsedMs).toBeLessThanOrEqual(2_200);
  expect(searchFramesPerSecond).toBeGreaterThanOrEqual(30);
  expect(interactionMs).toBeLessThan(750);
});

test("the complete board and primary controls fit a narrow viewport", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "Responsive geometry is covered once in Chromium.");
  await page.setViewportSize({ width: 390, height: 844 });
  await startGame(page, "easy", "human");
  await expect(page.getByRole("gridcell")).toHaveCount(63);
  const geometry = await page.evaluate(() => ({
    viewport: window.innerWidth,
    documentWidth: document.documentElement.scrollWidth,
    boardWidth: document.querySelector(".board")?.getBoundingClientRect().width ?? 0,
  }));
  expect(geometry.documentWidth).toBeLessThanOrEqual(geometry.viewport);
  expect(geometry.boardWidth).toBeGreaterThan(300);
  expect(geometry.boardWidth).toBeLessThanOrEqual(geometry.viewport);
  await expect(page.getByRole("button", { name: "New Game", exact: true })).toBeVisible();
});

test("AI vs AI stops after the active search and resumes only on request", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "The AI scheduling regression runs once in Chromium.");
  await startGame(page, "easy", "human");
  await page.getByRole("button", { name: "AI vs AI", exact: true }).click();
  await expect(page.getByText(/AI is thinking/)).toBeVisible();
  await page.getByRole("button", { name: "Stop AI vs AI" }).click();
  const historyAtStop = (await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.history.length)) ?? 0;
  await expect(page.getByRole("button", { name: "Resume AI vs AI" })).toBeVisible();
  await expect(page.getByText(/AI is thinking/)).toBeHidden({ timeout: 30_000 });
  const stoppedHistory = (await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.history.length)) ?? 0;
  expect(stoppedHistory).toBeGreaterThanOrEqual(historyAtStop);
  expect(stoppedHistory).toBeLessThanOrEqual(historyAtStop + 1);
  await page.waitForTimeout(800);
  expect(await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.history.length)).toBe(stoppedHistory);
  await page.getByRole("button", { name: "Resume AI vs AI" }).click();
  await expect.poll(
    async () => (await page.evaluate(() => window.__JUNGLE_TEST__?.getSnapshot()?.history.length)) ?? 0,
    { timeout: 30_000 },
  ).toBeGreaterThan(stoppedHistory);
  await page.getByRole("button", { name: "Stop AI vs AI" }).click();
});

test("UI completes a terminal den-entry fixture", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "Terminal UI fixture is covered once; runtime smoke runs in all engines.");
  await startGame(page);
  const save = positionSave([
    [10, { side: "blue", kind: "CAT" }],
    [62, { side: "red", kind: "RAT" }],
  ]);
  await page.evaluate(async (value) => window.__JUNGLE_TEST__?.importSave(value), save);
  await page.locator('[data-canonical-index="10"]').click();
  await page.locator('[data-canonical-index="3"]').click();
  await expect(page.getByRole("dialog", { name: "BLUE wins" })).toBeVisible();
  await expect(page.getByText("Blue entered the red den.")).toBeVisible();
});

test("Hard AI loads the exact tablebase and a deterministic game reaches a real terminal result", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "The deterministic tablebase game runs once in Chromium.");
  await startGame(page);
  const save = positionSave([
    [28, { side: "blue", kind: "CAT" }],
    [34, { side: "red", kind: "DOG" }],
  ]);
  await page.evaluate(async (value) => window.__JUNGLE_TEST__?.importSave(value), save);
  const result = await page.evaluate(async () => window.__JUNGLE_TEST__?.runAiGame(60, 100));
  expect(result?.maxTablebaseHits).toBeGreaterThan(0);
  expect(["den_entry", "capture_all", "no_legal_moves"]).toContain(result?.snapshot.gameState.result);
  expect(result?.snapshot.gameState.winner).not.toBeNull();
});
