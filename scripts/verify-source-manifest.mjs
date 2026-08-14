import { verifySourceManifest } from "./source-manifest-lib.mjs";

const manifest = await verifySourceManifest();
process.stdout.write(
  `Verified ${Object.keys(manifest.files).length} Python payload files from ${manifest.sourceCommit}.\n`,
);

