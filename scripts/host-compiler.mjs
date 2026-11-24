import { build } from "esbuild";
import { mkdirSync, copyFileSync, readdirSync } from "node:fs";
export async function bundleCompiler(out) {
  const vendor = "node_modules/@bytecodealliance/jco-transpile/vendor";
  mkdirSync(out + "/vendor", { recursive: true });
  for (const f of readdirSync(vendor).filter((f) => f.endsWith(".wasm")))
    copyFileSync(vendor + "/" + f, out + "/vendor/" + f);
  await build({
    entryPoints: ["conformance/compile.mjs"],
    outfile: out + "/compiler/bindgen.js",
    bundle: true,
    format: "esm",
    minify: true,
    platform: "browser",
    target: "es2022",
    define: { "process.versions": "undefined" },
    external: ["node:*"],
  });
}
