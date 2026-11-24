// A component custom section keeps the required artwork notices in the one
// file that is redistributed to another site.
function uleb(n) {
  const bytes = [];
  do {
    const part = n & 127;
    n >>>= 7;
    bytes.push(n ? part | 128 : part);
  } while (n);
  return Buffer.from(bytes);
}
export function withNotice(component, notice) {
  const name = Buffer.from("jungle-art-credits");
  const payload = Buffer.concat([uleb(name.length), name, Buffer.from(notice)]);
  return Buffer.concat([
    component,
    Buffer.from([0]),
    uleb(payload.length),
    payload,
  ]);
}
