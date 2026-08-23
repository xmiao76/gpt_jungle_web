import { expect, test, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import type { JungleSave, PieceData } from "../../src/game/types";

interface PublicPageAudit {
  errors: string[];
  responses: Set<string>;
}

function auditPage(page: Page): PublicPageAudit {
  const audit: PublicPageAudit = { errors: [], responses: new Set<string>() };
  page.on("pageerror", (error) => audit.errors.push(`pageerror: ${error.message}`));
  page.on("console", (message) => {
    if (message.type() === "error") audit.errors.push(`console: ${message.text()}`);
  });
  page.on("response", (response) => {
    audit.responses.add(response.url());
    if (response.status() >= 400) audit.errors.push(`${response.status()} ${response.url()}`);
  });
  return audit;
}

async function waitForReady(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Entering the jungle" })).toBeVisible({ timeout: 5_000 });
  await expect(page.getByRole("dialog", { name: "Start a new expedition" })).toBeVisible({ timeout: 90_000 });
}

async function begin(page: Page, difficulty: "Easy" | "Medium" | "Hard", starter: "Human starts" | "AI starts") {
  await page.getByText(difficulty, { exact: true }).click();
  await page.getByText(starter, { exact: true }).click();
  await page.getByRole("button", { name: "Begin game" }).click();
  await expect(page.getByRole("dialog", { name: "Start a new expedition" })).toBeHidden();
}

function twoPieceSave(): JungleSave {
  const board: Array<PieceData | null> = Array.from({ length: 63 }, () => null);
  board[28] = { side: "blue", kind: "CAT" };
  board[34] = { side: "red", kind: "DOG" };
  return {
    schema: "gpt-jungle-save",
    version: 1,
    engineVersion: "2.0.0",
    createdAt: "2026-08-13T00:00:00.000Z",
    gameState: {
      board,
      side_to_move: "blue",
      move_history: [],
      winner: null,
      result: "ongoing",
      result_reason: "",
    },
    settings: { difficulty: "hard", humanSide: "blue", boardFlipped: false },
  };
}

test("production assets and human-vs-AI controls work without test hooks", async ({ page }) => {
  const audit = auditPage(page);
  await waitForReady(page);
  await expect(page.getByRole("gridcell")).toHaveCount(63);
  await expect(page.locator(".piece")).toHaveCount(16);
  expect(await page.evaluate(() => "__JUNGLE_TEST__" in window)).toBe(false);
  await begin(page, "Easy", "Human starts");
  await page.getByRole("button", { name: "Diagnostics" }).click();
  await expect(page.locator(".diagnostics-card")).toContainText("Jungle Rust Engine 2.0.0");
  await expect(page.locator(".diagnostics-card")).toContainText("TablebaseLoaded");

  await page.locator('[data-canonical-index="42"]').click();
  await page.locator('[data-canonical-index="35"]').click();
  const playedHistory = page.locator(".move-history li:not(.empty-history)");
  await expect(playedHistory).toHaveCount(2, { timeout: 30_000 });

  const downloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  const download = await downloadPromise;
  const path = await download.path();
  expect(path).not.toBeNull();
  const saveText = await readFile(path!, "utf8");
  expect(JSON.parse(saveText)).toMatchObject({ schema: "gpt-jungle-save", version: 1 });

  await page.getByRole("button", { name: "Undo", exact: true }).click();
  await expect(playedHistory).toHaveCount(0);
  await page.getByRole("button", { name: "Redo", exact: true }).click();
  await expect(playedHistory).toHaveCount(2);
  await page.locator('input[type="file"]').setInputFiles({
    name: "production-round-trip.json",
    mimeType: "application/json",
    buffer: Buffer.from(saveText),
  });
  await expect(page.getByRole("status")).toContainText("Saved game loaded");

  await page.getByRole("button", { name: "Flip Board" }).click();
  await expect(page.locator('[data-display-index="20"]')).toHaveAttribute("data-canonical-index", "42");
  await expect(page.locator(".diagnostics-card")).toContainText("Loaded");
  await page.getByRole("button", { name: "Mute" }).click();
  await expect(page.getByRole("button", { name: "Unmute" })).toHaveAttribute("aria-pressed", "true");

  await page.setViewportSize({ width: 390, height: 844 });
  const width = await page.evaluate(() => ({ viewport: innerWidth, content: document.documentElement.scrollWidth }));
  expect(width.content).toBeLessThanOrEqual(width.viewport);
  expect(audit.errors).toEqual([]);

  const paths = [...audit.responses].map((url) => new URL(url).pathname);
  expect(paths.some((path) => /\/assets\/.*\.js$/.test(path))).toBe(true);
  expect(paths.some((path) => /\/assets\/.*\.css$/.test(path))).toBe(true);
  expect(paths.some((path) => /\.png$/.test(path))).toBe(true);
  expect(paths.some((path) => path.endsWith("/generated/engine-runtime-manifest.json"))).toBe(true);
  expect(paths.some((path) => /\/generated\/jungle-wasm\/[^/]+\/jungle_wasm_bg\.wasm$/.test(path))).toBe(true);
  expect(paths.some((path) => path.endsWith("/generated/tablebase/two_piece_v1.jgtb"))).toBe(true);
});

test("production AI-first remains responsive and assigns the human Red", async ({ page }) => {
  const audit = auditPage(page);
  await waitForReady(page);
  await begin(page, "Easy", "AI starts");
  await expect(page.locator(".move-history li:not(.empty-history)")).toHaveCount(1, { timeout: 30_000 });
  await expect(page.locator(".status-card")).toContainText("You: Red");
  await page.getByRole("button", { name: "How to Play" }).click();
  await expect(page.getByRole("dialog", { name: "How to play" })).toBeVisible();
  await page.getByRole("button", { name: "Ready to play" }).click();
  expect(audit.errors).toEqual([]);
});

test("production observes a complete authentic tablebase game", async ({ page }) => {
  const audit = auditPage(page);
  await waitForReady(page);
  await page.locator('input[type="file"]').setInputFiles({
    name: "two-piece-tablebase.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify(twoPieceSave())),
  });
  await expect(page.getByRole("status")).toContainText("Saved game loaded");
  await page.getByRole("button", { name: "Diagnostics" }).click();
  await expect(page.locator(".diagnostics-card")).toContainText("TablebaseLoaded");
  await page.getByRole("button", { name: "AI vs AI", exact: true }).click();
  const result = page.getByRole("dialog", { name: /wins/i });
  await expect(result).toBeVisible({ timeout: 90_000 });
  await expect(result).toContainText(/entered the .* den|captured every opposing animal|has no legal move/i);
  expect(await page.locator(".move-history li:not(.empty-history)").count()).toBeGreaterThan(0);
  expect(audit.errors).toEqual([]);
});
