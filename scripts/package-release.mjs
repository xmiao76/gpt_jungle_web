import {
  readFileSync,
  writeFileSync,
  copyFileSync,
  mkdirSync,
  cpSync,
} from "node:fs";
import { createHash } from "node:crypto";
const meta = JSON.parse(readFileSync("generated/metadata.json"));
let html = readFileSync("dist/index.html", "utf8");
html = html.replace(
  /<script\b[^>]*\bsrc="([^"]+)"[^>]*><\/script>/g,
  (_, src) =>
    '<script type="module">' +
    readFileSync("dist/" + src, "utf8").replace(/<\/script/gi, "<\\/script") +
    "</script>",
);
html = html.replace(
  /<link\b[^>]*rel="stylesheet"[^>]*href="([^"]+)"[^>]*>/g,
  (_, src) => "<style>" + readFileSync("dist/" + src, "utf8") + "</style>",
);
const notices = [
  "docs/art-license.txt",
  "node_modules/react/LICENSE",
  "node_modules/react-dom/LICENSE",
  "node_modules/scheduler/LICENSE",
]
  .map((p) => readFileSync(p, "utf8"))
  .join("\n");
const credits =
  '<script type="text/plain" id="redistribution-notices">' +
  notices.replace(/<\/script/gi, "<\\/script") +
  "</script>";
html = html.replace("</body>", () => credits + "</body>").replace(/\r\n/g, "\n");
const hosted = readFileSync("dist/index.html", "utf8");
if (!hosted.includes("redistribution-notices"))
  writeFileSync(
    "dist/index.html",
    hosted.replace("</body>", () => credits + "</body>"),
  );
const digest = createHash("sha256").update(html).digest("hex");
const sums = meta.sha256 + "  " + meta.filename + "\n" + digest + "  index.html\n";
const component = readFileSync("generated/" + meta.filename);
if (createHash("sha256").update(component).digest("hex") !== meta.sha256 ||
    !html.includes(component.toString("base64")))
  throw Error("Offline component and staged release differ");
writeFileSync("dist/" + meta.filename, component);
mkdirSync("dist/release", { recursive: true });
writeFileSync("dist/release/index.html", html);
writeFileSync("dist/SHA256.txt", sums.replace("  index.html", "  release/index.html"));
copyFileSync("public/_headers", "dist/_headers");
mkdirSync("dist/conformance", { recursive: true });
cpSync("conformance", "dist/conformance", { recursive: true });
for (const file of ["contract.wasm", "first-legal.wasm"])
  copyFileSync("generated/" + file, "dist/conformance/" + file);
copyFileSync("generated/metadata.json", "dist/conformance/metadata.json");
copyFileSync("wit/jungle.wit", "dist/conformance/jungle.wit");
const readme = readFileSync("README.md", "utf8");
const start = "<!-- release -->",
  end = "<!-- /release -->";
if (!readme.includes(start) || !readme.includes(end))
  throw Error("Missing README release markers");
const block = [
  start,
  "",
  meta.title + " · " + meta.gameVersion,
  "",
  "- Model " +
    meta.model +
    ", code agent " +
    meta.codeAgent +
    ", platform " +
    meta.modelPlatform +
    ".",
  "- Engine " + meta.engineVersion + ", " + meta.runtime + ".",
  "- Source " + meta.githubUrl,
  "- Rules `" + meta.rulesId + "` · `" + meta.rulesDigest + "`",
  "- `" + meta.filename + "` · SHA-256 `" + meta.sha256 + "`",
  "- `release/index.html` · SHA-256 `" + digest + "`",
  "",
  end,
].join("\n");
mkdirSync("release", { recursive: true });
writeFileSync("release/index.html", html);
copyFileSync("dist/" + meta.filename, "release/" + meta.filename);
writeFileSync("release/SHA256.txt", sums);
for (const file of ["contract.wasm", "first-legal.wasm"])
  copyFileSync("dist/conformance/" + file, "conformance/" + file);
writeFileSync(
  "README.md",
  readme.slice(0, readme.indexOf(start)) +
    block +
    readme.slice(readme.indexOf(end) + end.length),
);
