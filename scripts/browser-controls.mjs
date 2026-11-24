import assert from "node:assert/strict";
import { chromium } from "playwright";
import { preview } from "vite";
import { writeFileSync } from "node:fs";
const server = await preview({
  preview: { port: 0, host: "127.0.0.1", open: false },
});
const url = "http://127.0.0.1:" + server.httpServer.address().port;
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1240, height: 890 } });
const screenshots = "docs/evidence/screens/";
const getButton = (name) => page.getByRole("button", { name, exact: true });
const rev = () => page.locator(".game-layout").getAttribute("data-revision");
const waitRevision = (n) =>
  page.waitForFunction(
    (n) =>
      document.querySelector(".game-layout")?.dataset.revision === String(n),
    n,
  );
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
try {
  // Hold worker startup just long enough to photograph the real loading screen.
  await page.addInitScript(() => {
    const WorkerClass = window.Worker;
    window.Worker = class extends WorkerClass {
      postMessage(...args) {
        if (args[0]?.engine) setTimeout(() => super.postMessage(...args), 300);
        else super.postMessage(...args);
      }
    };
  });
  await page.goto(url, { waitUntil: "commit" });
  await page
    .locator(".loading-screen")
    .screenshot({ path: screenshots + "loading.png" });
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("Difficulty").selectOption("hard");
  await getButton("Begin game").click();
  await page.locator('[data-canonical-index="42"]').click();
  await page.locator('[data-canonical-index="35"]').click();
  await waitRevision(1);
  await getButton("Undo").click();
  await waitRevision(0);
  await sleep(1100);
  assert.equal(await rev(), "0");
  await getButton("Redo").click();
  await waitRevision(1);
  await getButton("Restart").click();
  await waitRevision(0);
  await sleep(1100);
  assert.equal(await rev(), "0");
  await getButton("Flip board").click();
  assert.equal(
    await page
      .locator('[data-display-index="20"]')
      .getAttribute("data-canonical-index"),
    "42",
  );
  await getButton("Mute").click();
  await getButton("Unmute").waitFor();
  await page
    .locator("input[type=file]")
    .setInputFiles({
      name: "old.json",
      mimeType: "application/json",
      buffer: Buffer.from(
        JSON.stringify({ schema: "gpt-jungle-save", version: 1 }),
      ),
    });
  await page
    .getByRole("alert")
    .filter({ hasText: "earlier Jungle ruleset" })
    .waitFor();
  assert.equal(await rev(), "0");
  await getButton("Dismiss").click();
  await getButton("New game").click();
  await dialog.getByLabel("Game mode").selectOption("auto");
  await dialog.getByLabel("First color").selectOption("blue");
  await dialog.getByLabel("Difficulty").selectOption("medium");
  await getButton("Begin game").click();
  await waitRevision(1);
  await getButton("Undo").click();
  await waitRevision(0);
  await sleep(850);
  assert.equal(await rev(), "0");
  await getButton("Resume").click();
  await waitRevision(1);
  await getButton("Pause").click();
  const n = await rev();
  await sleep(850);
  assert.equal(await rev(), n);
  const [save] = await Promise.all([
    page.waitForEvent("download"),
    getButton("Save").click(),
  ]);
  await getButton("Restart").click();
  await page.locator("input[type=file]").setInputFiles(await save.path());
  await waitRevision(Number(n));
  await getButton("Resume").waitFor();
  await getButton("Diagnostics").click();
  await page.screenshot({ path: screenshots + "diagnostics.png" });
  const data = await page.evaluate(() => ({
    side: document.querySelector(".game-layout").dataset.side,
    mode: document.querySelector(".game-layout").dataset.runtime,
  }));
  writeFileSync(
    "docs/evidence/controls.json",
    JSON.stringify(
      {
        browser: browser.version(),
        ...data,
        passed: [
          "undo during search",
          "redo",
          "restart during search",
          "flip mapping",
          "mute",
          "v1 rejection",
          "Blue first",
          "AI vs AI undo and pause",
          "v2 save reload",
        ],
      },
      null,
      2,
    ) + "\n",
  );
  console.log("Control interactions and stale request checks passed.");
} finally {
  await browser.close();
  await new Promise((r) => server.httpServer.close(r));
}
