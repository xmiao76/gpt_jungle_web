import assert from "node:assert/strict";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { createHash } from "node:crypto";
import { chromium, firefox, webkit } from "playwright";
import { preview } from "vite";
import { legal, Referee } from "../conformance/referee.mjs";
import { initSync, search } from "../generated/standalone.js";
const flags = process.argv.slice(2),
  flag = (key, def) =>
    flags.includes(key) ? flags[flags.indexOf(key) + 1] : def;
const target = flag("--target", "all"),
  names = flag("--browsers", "chrome,edge,firefox,webkit").split(",");
const meta = JSON.parse(readFileSync("generated/metadata.json"));
const evidence = "docs/evidence";
mkdirSync(evidence + "/screens", { recursive: true });
initSync({ module: readFileSync("generated/standalone_bg.wasm") });
const server = process.env.PLAYWRIGHT_BASE_URL
  ? null
  : await preview({ preview: { host: "127.0.0.1", port: 0, open: false } });
const url =
  process.env.PLAYWRIGHT_BASE_URL ??
  "http://127.0.0.1:" + server.httpServer.address().port;
const file = pathToFileURL(resolve("release/index.html")).href;
const results = [];
const digest = (b) => createHash("sha256").update(b).digest("hex");
async function shot(page, name) {
  await page.screenshot({
    path: evidence + "/screens/" + name + ".png",
    animations: "disabled",
    timeout: 10000,
  });
}
async function setup(
  page,
  {
    mode = "human",
    difficulty = "easy",
    first = "red",
    starter = "human",
  } = {},
) {
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("Game mode").selectOption(mode);
  await dialog.getByLabel("Difficulty").selectOption(difficulty);
  await dialog.getByLabel("First color").selectOption(first);
  if (mode === "human")
    await dialog.getByLabel("First mover").selectOption(starter);
  await dialog.getByRole("button", { name: "Begin game" }).click();
}
async function snapshot(page) {
  return page.locator(".game-layout").evaluate((el) => ({
    rev: Number(el.dataset.revision),
    side: el.dataset.side,
    status: el.dataset.status,
    reason: el.dataset.reason,
    moves: [...document.querySelectorAll(".move-history li small")]
      .reverse()
      .map((s) => {
        const m = s.textContent.match(/^(\d+) → (\d+)/);
        return { src: +m[1], dst: +m[2] };
      }),
  }));
}
async function download(page) {
  const action = page
    .getByRole("dialog")
    .getByRole("button", { name: "Download Arena .wasm" });
  const [d] = await Promise.all([
    page.waitForEvent("download"),
    action.click(),
  ]);
  assert.equal(d.suggestedFilename(), meta.filename);
  const bytes = readFileSync(await d.path());
  assert.equal(digest(bytes), meta.sha256);
  return bytes;
}
async function humanGame(page, record, photos, level = "easy") {
  const ref = new Referee();
  await setup(page, { difficulty: level });
  while (ref.state.status === "playing") {
    let s = await snapshot(page);
    if (
      s.rev === ref.state.rev &&
      s.side === "blue" &&
      s.status === "playing"
    ) {
      await page.waitForFunction(
        (n) =>
          Number(document.querySelector(".game-layout").dataset.revision) > n,
        s.rev,
        { timeout: 6000 },
      );
      s = await snapshot(page);
    }
    for (const m of s.moves.slice(ref.state.rev)) ref.apply(m);
    assert.equal(s.status, ref.state.status);
    if (s.status !== "playing") break;
    const moves = legal(ref.state),
      pick = JSON.parse(
        search(JSON.stringify(ref.state), "hard", 13, 1200, 0),
      ).index;
    const m = moves[pick];
    await page
      .locator('[data-canonical-index="' + m.src + '"]')
      .click({ force: true });
    if (photos && ref.state.rev === 0) await shot(page, "selected-move");
    await page
      .locator('[data-canonical-index="' + m.dst + '"]')
      .click({ force: true });
    await page.waitForFunction(
      (n) =>
        Number(document.querySelector(".game-layout").dataset.revision) > n,
      s.rev,
      { timeout: 4000 },
    );
  }
  record.humanGame = {
    plies: ref.state.rev,
    reason: ref.state.reason,
    winner: ref.state.winner,
  };
  if (photos) {
    await page.getByRole("button", { name: "Review board" }).waitFor();
    await shot(page, "standalone-result");
  }
  record[level] = await timings(page, level);
  if (level === "hard")
    assert(record.hard.maxMs <= 1000, "Hard reply exceeded one second");
}
async function timings(page, level) {
  return page.evaluate((level) => {
    const times = performance
      .getEntriesByName("jungle.ai." + level)
      .map((e) => e.duration);
    return {
      moves: times.length,
      maxMs: Math.max(0, ...times),
      meanMs: times.reduce((a, b) => a + b, 0) / Math.max(1, times.length),
    };
  }, level);
}
async function webGame(
  browser,
  name,
  location,
  photos = false,
  blocked = false,
) {
  const ctx = await browser.newContext({
    viewport: { width: 1280, height: 920 },
    acceptDownloads: true,
  });
  const page = await ctx.newPage();
  const errors = [],
    external = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  if (location === file)
    await ctx.route(/^https?:/, (route) => {
      external.push(route.request().url());
      return route.abort();
    });
  if (blocked)
    await page.addInitScript(() => {
      globalThis.Worker = class {
        constructor() {
          throw new Error("Workers disabled for fallback test");
        }
      };
    });
  const start = Date.now();
  try {
    await page.goto(location, { waitUntil: "domcontentloaded" });
    await page.getByRole("dialog").waitFor({ timeout: 12000 });
    const r = {
      componentHash: meta.sha256,
      htmlHash:
        location === file
          ? digest(readFileSync("release/index.html"))
          : undefined,
      browser: name,
      version: browser.version(),
      target: location === file ? "file" : "hosted",
      origin: location === file ? undefined : new URL(location).origin,
      forcedFallback: blocked,
      coldLoadMs: Date.now() - start,
      runtime: await page.locator(".game-layout").getAttribute("data-runtime"),
    };
    results.push(r);
    if (blocked) assert.equal(r.runtime, "main thread");
    const downloaded = await download(page);
    if (photos) {
      await shot(page, "setup");
      await page
        .getByRole("dialog")
        .getByRole("button", { name: "How to play" })
        .click();
      await page.getByRole("button", { name: "Ready to play" }).waitFor();
      await shot(page, "how-to-play");
      await page.getByRole("button", { name: "Ready to play" }).click();
    }
    if (blocked && target !== "fallback-game") {
      await setup(page, { difficulty: "medium", starter: "ai", first: "blue" });
      await page.getByRole("button", { name: "Flip board" }).click();
      await page.waitForFunction(
        () => document.querySelector(".game-layout").dataset.revision === "1",
      );
      r.medium = await timings(page, "medium");
      assert.equal((await snapshot(page)).side, "red");
    } else if (target === "hard") {
      await setup(page, { mode: "auto", difficulty: "hard" });
      await page.waitForFunction(
        () =>
          document.querySelector(".game-layout").dataset.status !== "playing",
        null,
        { timeout: 900000 },
      );
      r.hard = await timings(page, "hard");
      r.game = await snapshot(page);
      assert.equal(r.hard.moves, r.game.rev);
      assert(r.hard.maxMs <= 1000, "Hard exceeded 1 second: " + r.hard.maxMs);
    } else {
      await humanGame(
        page,
        r,
        photos,
        target === "human-hard" ? "hard" : "easy",
      );
    }
    assert.deepEqual(errors, []);
    assert.deepEqual(external, []);
    console.log(
      name,
      r.target,
      r.coldLoadMs,
      r.runtime,
      r.humanGame ?? r.hard ?? r.medium,
    );
    return downloaded;
  } finally {
    await ctx.close();
  }
}
async function host(browser, name, downloaded) {
  const page = await browser.newPage({
    viewport: { width: 1120, height: 900 },
  });
  try {
    await page.goto(url + "/conformance/");
    if (downloaded)
      await page.locator("#file").setInputFiles({
        name: meta.filename, mimeType: "application/wasm", buffer: downloaded,
      });
    const value = await page.evaluate(async () => {
      const { loadDefault, admission, smoke, match, Seat } = await import(
        "./host.mjs"
      );
      const { wire } = await import("./referee.mjs");
      const { file, fixture } = await loadDefault();
      const selected = document.querySelector("#file").files[0];
      const entry = await admission(selected ? new Uint8Array(await selected.arrayBuffer()) : file),
        other = await admission(fixture),
        info = await smoke(entry);
      const games = [];
      const canvases = document.querySelectorAll("canvas");
      for (const [red, blue] of [
        [entry, other],
        [other, entry],
        [entry, entry],
      ]) {
        let n = 0;
        const log = await match(red, blue, (s, frames) => {
          if (s.rev !== n++) throw Error("Missed revision");
          frames.forEach((f, i) =>
            canvases[i].getContext("2d")
              .putImageData(new ImageData(f, 448, 576), 0, 0),
          );
        });
        const end = log.at(-1);
        if (!["won", "drawn"].includes(end.status))
          throw Error("Match ended by " + end.reason);
        games.push({
          plies: end.rev,
          reason: end.reason,
          winner: end.winner,
          renderedPerSeat: n,
        });
      }
      const s = new Seat();
      await s.call("open", entry);
      await s.call("init", {
        seat: "blue",
        seed: 20261120,
        difficulty: "medium",
      });
      const { opening } = await import("./referee.mjs");
      for (const [i, flip] of [false, true].entries()) {
        const f = await s.call("frame", wire(opening()), flip, 1000);
        canvases[i].getContext("2d")
          .putImageData(new ImageData(f, 448, 576), 0, 0);
      }
      s.close();
      return { hash: entry.hash, title: info.title, games };
    });
    assert.equal(value.hash, meta.sha256);
    if (downloaded) {
      const response = await page.request.get(url + "/" + meta.filename);
      assert.equal(response.status(), 200);
      assert.match(response.headers()["content-type"], /^application\/wasm\b/i);
      assert.equal(digest(await response.body()), value.hash);
      const offline = await page.request.get(url + "/release/");
      assert.equal(offline.status(), 200);
      const offlineHash = digest(await offline.body());
      assert.equal(offlineHash, digest(readFileSync("release/index.html")));
      value.publicDownloads = { contentType: response.headers()["content-type"], offlineHash };
    }
    results.push({ browser: name, host: url, ...value });
    await shot(page, "arena-orientations");
    console.log("Conformance", value.games);
  } finally {
    await page.close();
  }
}
try {
  for (const name of names) {
    const kind = {
      chrome: chromium,
      edge: chromium,
      chromium,
      firefox,
      webkit,
    }[name];
    const browser = await kind.launch({
      channel:
        name === "chrome" ? "chrome" : name === "edge" ? "msedge" : undefined,
      headless: true,
    });
    try {
      let downloaded;
      if (["all", "hosted", "public", "hard", "human-hard"].includes(target))
        downloaded = await webGame(
          browser,
          name,
          url + "/",
          name === "chrome" && target !== "hard",
        );
      if (["all", "file"].includes(target)) await webGame(browser, name, file);
      if (["all", "fallback", "fallback-game"].includes(target))
        await webGame(browser, name, file, false, true);
      if (target === "host" || (["all", "public"].includes(target) && name === names[0]))
        await host(browser, name, target === "public" ? downloaded : undefined);
    } finally {
      await browser.close();
    }
  }
} finally {
  writeFileSync(
    evidence +
      "/browser-" +
      (process.env.PLAYWRIGHT_BASE_URL && target !== "public" ? "public-" : "") +
      target +
      "-" +
      names.join("-") +
      ".json",
    JSON.stringify(results, null, 2) + "\n",
  );
  if (server) await new Promise((resolve) => server.httpServer.close(resolve));
}
