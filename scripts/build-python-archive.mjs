import { createHash } from "node:crypto";
import { mkdir, readFile, readdir, unlink, writeFile } from "node:fs/promises";
import path from "node:path";
import JSZip from "jszip";
import {
  posixRelative,
  pythonRoot,
  repositoryRoot,
  verifySourceManifest,
  walkFiles,
} from "./source-manifest-lib.mjs";

const manifest = await verifySourceManifest();
const outputDirectory = path.join(repositoryRoot, "public", "generated");
await mkdir(outputDirectory, { recursive: true });

for (const entry of await readdir(outputDirectory, { withFileTypes: true })) {
  if (entry.isFile() && (/^python-runtime\.[a-f0-9]{16}\.zip$/.test(entry.name) || entry.name === "python-runtime.json")) {
    await unlink(path.join(outputDirectory, entry.name));
  }
}

const zip = new JSZip();
const fixedDate = new Date("1980-01-01T00:00:00.000Z");
const archiveInputs = [
  ...(await walkFiles(path.join(pythonRoot, "jungle"))),
  path.join(pythonRoot, "browser_bridge.py"),
];

for (const file of archiveInputs.sort((left, right) => left.localeCompare(right))) {
  const archivePath = posixRelative(pythonRoot, file);
  zip.file(archivePath, await readFile(file), {
    date: fixedDate,
    createFolders: false,
    unixPermissions: 0o644,
  });
}

const archive = await zip.generateAsync({
  type: "nodebuffer",
  compression: "DEFLATE",
  compressionOptions: { level: 9 },
  platform: "UNIX",
  streamFiles: true,
});
const digest = createHash("sha256").update(archive).digest("hex");
const archiveName = `python-runtime.${digest.slice(0, 16)}.zip`;
await writeFile(path.join(outputDirectory, archiveName), archive);
await writeFile(
  path.join(outputDirectory, "python-runtime.json"),
  `${JSON.stringify(
    {
      schema: "gpt-jungle-python-runtime",
      version: 1,
      file: archiveName,
      sha256: digest,
      bytes: archive.byteLength,
      pyodideVersion: "314.0.3",
      sourceCommit: manifest.sourceCommit,
    },
    null,
    2,
  )}\n`,
  "utf8",
);
process.stdout.write(`Built ${archiveName} (${archive.byteLength.toLocaleString()} bytes).\n`);
