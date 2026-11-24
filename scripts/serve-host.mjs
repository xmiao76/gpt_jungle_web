import {
  cpSync,
  copyFileSync,
  mkdirSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "vite";
import { bundleCompiler } from "./host-compiler.mjs";
const dir = "generated/review";
mkdirSync(dir + "/conformance", { recursive: true });
cpSync("conformance", dir + "/conformance", { recursive: true });
const name = readFileSync("release/SHA256.txt", "utf8")
  .split(/\r?\n/)[0]
  .split("  ")[1];
copyFileSync("release/" + name, dir + "/" + name);
copyFileSync("release/index.html", dir + "/index.html");
writeFileSync(
  dir + "/conformance/metadata.json",
  JSON.stringify({ filename: name }),
);
await bundleCompiler(dir + "/conformance");
const server = await createServer({
  configFile: false,
  root: dir,
  publicDir: false,
  server: { host: "127.0.0.1" },
});
await server.listen();
console.log(server.resolvedUrls.local[0] + "conformance/");
