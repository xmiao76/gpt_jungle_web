import { admission, loadDefault, match, Seat, smoke } from "./host.mjs";
import { wire } from "./referee.mjs";
const el = (id) => document.getElementById(id),
  canvases = [...document.querySelectorAll("canvas")],
  status = el("status"),
  seek = el("seek");
let history = [],
  participants = [],
  replaySeats = [];
function draw(s, frames) {
  frames.forEach((f, i) =>
    canvases[i].getContext("2d").putImageData(new ImageData(f, 448, 576), 0, 0),
  );
  status.textContent =
    "Revision " + s.rev + " · " + (s.reason || s.toPlay + " to move");
}
el("run").onclick = async () => {
  el("run").disabled = true;
  seek.disabled = true;
  replaySeats.forEach((s) => s.close());
  replaySeats = [];
  try {
    status.textContent = "Checking component and contract…";
    const { file, fixture } = await loadDefault(),
      upload = el("file").files[0];
    const release = await admission(
        upload ? new Uint8Array(await upload.arrayBuffer()) : file,
      ),
      other = await admission(fixture);
    const info = await smoke(release),
      pair = el("pair").value;
    participants =
      pair === "self"
        ? [release, release]
        : pair === "red"
          ? [release, other]
          : [other, release];
    history = await match(...participants, draw);
    seek.max = String(history.length - 1);
    seek.value = seek.max;
    replaySeats = await Promise.all(
      participants.map(async (c, i) => {
        const s = new Seat();
        await s.call("open", c);
        await s.call("init", {
          seat: i ? "blue" : "red",
          difficulty: "medium",
          seed: 20261120,
        });
        return s;
      }),
    );
    seek.disabled = false;
    status.textContent += " · " + info.title + " · SHA-256 " + release.hash;
  } catch (e) {
    status.textContent = String(e);
  } finally {
    el("run").disabled = false;
  }
};
seek.onchange = async () => {
  seek.disabled = true;
  try {
    const s = history[Number(seek.value)];
    draw(
      s,
      await Promise.all(
        replaySeats.map((seat, i) =>
          seat.call("frame", wire(s), Boolean(i), 1000),
        ),
      ),
    );
  } finally {
    seek.disabled = false;
  }
};
