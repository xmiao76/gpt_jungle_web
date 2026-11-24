import { instantiate } from "./seat.mjs";
let seat;
self.onmessage = async ({ data: { id, method, args } }) => {
  try {
    let value;
    if (method === "open") {
      seat = await instantiate(args[0], async (source) => {
        const url = URL.createObjectURL(
          new Blob([source], { type: "text/javascript" }),
        );
        try {
          return await import(url);
        } finally {
          URL.revokeObjectURL(url);
        }
      });
      value = seat.info();
    } else value = seat[method](...args);
    self.postMessage({ id, value }, method === "frame" ? [value.buffer] : []);
  } catch (e) {
    self.postMessage({ id, error: String(e) });
  }
};
