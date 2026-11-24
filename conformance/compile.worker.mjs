import { compile } from "./compiler/bindgen.js";
self.onmessage = async ({ data }) => {
  try {
    self.postMessage({ value: await compile(data.bytes, data.reference) });
  } catch (e) {
    self.postMessage({ error: String(e) });
  }
};
