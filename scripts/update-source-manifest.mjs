import { writeFile } from "node:fs/promises";
import { currentSourceHashes, manifestPath } from "./source-manifest-lib.mjs";

const manifest = {
  schema: "gpt-jungle-source-manifest",
  version: 1,
  sourceRepository: "https://github.com/xmiao76/gpt_cx_jungle.git",
  sourceCommit: "b0bb427458a686aa0ed10770bda50f9311a7845b",
  sourceRoot: "src/jungle",
  files: await currentSourceHashes(),
};

await writeFile(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
process.stdout.write(`Updated ${manifestPath} with ${Object.keys(manifest.files).length} source files.\n`);

