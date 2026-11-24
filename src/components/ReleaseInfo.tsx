import meta from "../../generated/metadata.json";
import { entrant } from "../../generated/embedded";
import { decode } from "../game/runtime";
export function download(bytes: BlobPart, filename: string, type: string) {
  const url = URL.createObjectURL(new Blob([bytes], { type }));
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 30000);
}
export function ReleaseInfo() {
  return (
    <section className="release-info" aria-label="Arena release">
      <strong>
        {meta.title} <small>v{meta.gameVersion}</small>
      </strong>
      <div>
        Model {meta.model} · Agent {meta.codeAgent}
        {meta.modelPlatform && " · Platform " + meta.modelPlatform}
      </div>
      <div>
        {meta.runtime} · Engine {meta.engineVersion} ·{" "}
        <a href={meta.githubUrl} target="_blank" rel="noreferrer">
          GitHub source
        </a>
      </div>
      <button
        type="button"
        className="secondary-button"
        onClick={() =>
          download(decode(entrant), meta.filename, "application/wasm")
        }
      >
        Download Arena .wasm
      </button>
      <details>
        <summary>Upload instructions and file details</summary>
        <p>
          Open the{" "}
          <a
            href="https://jungle-leaderboard.pages.dev/arena/upload/"
            target="_blank"
            rel="noreferrer"
          >
            Arena Upload page
          </a>
          , choose the downloaded file, and wait for every admission stage to
          pass. Select <b>Play in left seat</b> or <b>Play in right seat</b>.
        </p>
        <dl>
          <dt>File</dt>
          <dd>{meta.filename}</dd>
          <dt>SHA-256</dt>
          <dd>{meta.sha256}</dd>
          <dt>Rules</dt>
          <dd>{meta.rulesId}</dd>
          <dt>Rules SHA-256</dt>
          <dd>{meta.rulesDigest}</dd>
        </dl>
      </details>
    </section>
  );
}
