import { createHash } from "node:crypto";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const pythonRoot = path.join(repositoryRoot, "python");
export const jungleRoot = path.join(pythonRoot, "jungle");
export const manifestPath = path.join(pythonRoot, "source-manifest.json");

export async function walkFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries.sort((left, right) => left.name.localeCompare(right.name))) {
    if (entry.name === "__pycache__" || entry.name.endsWith(".pyc") || entry.name.endsWith(".pyo")) {
      continue;
    }
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await walkFiles(absolute)));
    } else if (entry.isFile()) {
      files.push(absolute);
    }
  }
  return files;
}

export function posixRelative(base, file) {
  return path.relative(base, file).split(path.sep).join("/");
}

export async function sha256File(file) {
  const bytes = await readFile(file);
  return createHash("sha256").update(bytes).digest("hex");
}

export async function currentSourceHashes() {
  const files = await walkFiles(jungleRoot);
  const hashes = {};
  for (const file of files) {
    hashes[`jungle/${posixRelative(jungleRoot, file)}`] = await sha256File(file);
  }
  return hashes;
}

export async function verifySourceManifest() {
  const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
  const actual = await currentSourceHashes();
  const expectedNames = Object.keys(manifest.files).sort();
  const actualNames = Object.keys(actual).sort();
  const problems = [];

  for (const name of new Set([...expectedNames, ...actualNames])) {
    if (!(name in manifest.files)) {
      problems.push(`untracked source file: ${name}`);
    } else if (!(name in actual)) {
      problems.push(`missing source file: ${name}`);
    } else if (manifest.files[name] !== actual[name]) {
      problems.push(`hash mismatch: ${name}`);
    }
  }
  if (problems.length > 0) {
    throw new Error(`Python source manifest verification failed:\n${problems.join("\n")}`);
  }
  return manifest;
}
