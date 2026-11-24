import { generate, $init } from "@bytecodealliance/jco-transpile/component";
import { componentWit } from "@bytecodealliance/jco-transpile/wasm-tools";
export async function compile(bytes, reference) {
  if (bytes.length > 25165824) throw Error("Component exceeds 24 MiB");
  if ([0, 97, 115, 109, 13, 0, 1, 0].some((v, i) => bytes[i] !== v))
    throw Error("Expected a WebAssembly component");
  const hash = Array.from(
    new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)),
    (b) => b.toString(16).padStart(2, "0"),
  ).join("");
  const actual = await componentWit(bytes),
    expected = await componentWit(reference);
  if (actual !== expected)
    throw Error(
      "Expanded WIT contract differs from jungle:arena/competitor@1.0.0",
    );
  await $init;
  const generated = generate(bytes, {
    name: "entrant",
    instantiation: { tag: "sync" },
    noNodejsCompat: true,
    noTypescript: true,
    base64Cutoff: 0,
  });
  const modules = {},
    decoder = new TextDecoder();
  let source = "";
  for (const [name, body] of generated.files) {
    if (name.endsWith(".wasm")) modules[name] = await WebAssembly.compile(body);
    if (name.endsWith(".js")) source = decoder.decode(body);
  }
  return { source, modules, hash, imports: generated.imports };
}
