import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync, mkdirSync, copyFileSync } from "node:fs";
import { withNotice } from "./component-notice.mjs";
import { resolve } from "node:path";

const root = process.cwd();
const generated = resolve(root, "generated");
mkdirSync(generated, { recursive: true });
const manifest = ["--manifest-path", "engine/Cargo.toml"];
const wasm = "engine/target/wasm32-unknown-unknown/release/jungle_entry.wasm";
const run = (cmd, args) => execFileSync(cmd, args, { stdio: "inherit" });
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const meta = JSON.parse(
  execFileSync(
    "cargo",
    [
      "run",
      "--locked",
      "--quiet",
      ...manifest,
      "-p",
      "jungle-entry",
      "--example",
      "metadata",
    ],
    { encoding: "utf8" },
  ),
);
if (sha(readFileSync("rules/desktop-jungle-v2.json")) !== meta.rulesDigest)
  throw Error("Rules digest mismatch");
for (const feature of ["arena", "fixture", "web"]) {
  run("cargo", [
    "build",
    "--release",
    "--locked",
    ...manifest,
    "--target",
    "wasm32-unknown-unknown",
    "-p",
    "jungle-entry",
    "--features",
    feature,
  ]);
  if (feature === "web") {
    run("wasm-bindgen", [
      wasm,
      "--target",
      "web",
      "--out-dir",
      generated,
      "--out-name",
      "standalone",
    ]);
  } else {
    const output =
      feature === "arena"
        ? "generated/" + meta.filename
        : "generated/first-legal.wasm";
    run("wasm-tools", ["component", "new", wasm, "-o", output]);
    writeFileSync(
      output,
      withNotice(
        readFileSync(output),
        readFileSync("docs/art-license.txt", "utf8"),
      ),
    );
    run("wasm-tools", ["validate", output]);
  }
}
const entrant = readFileSync("generated/" + meta.filename);
if (
  entrant.length > 24 * 1024 * 1024 ||
  !entrant.subarray(0, 8).equals(Buffer.from("0061736d0d000100", "hex"))
)
  throw Error("Invalid release component");
meta.sha256 = sha(entrant);
writeFileSync("generated/metadata.json", JSON.stringify(meta, null, 2) + "\n");
writeFileSync(
  "generated/embedded.ts",
  "export const entrant = " +
    JSON.stringify(entrant.toString("base64")) +
    ";\nexport const engine = " +
    JSON.stringify(
      readFileSync("generated/standalone_bg.wasm").toString("base64"),
    ) +
    ";\n",
);
copyFileSync("wit/jungle.wit", "generated/jungle.wit");
