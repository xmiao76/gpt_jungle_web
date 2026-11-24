export async function instantiate(compiled, loadSource) {
  const { instantiate } = await loadSource(compiled.source);
  let frames = [];
  function present(width, height, rgba) {
    if (width !== 448 || height !== 576 || rgba.length !== 1032192)
      throw Error("Invalid frame geometry");
    if (frames.length) throw Error("Multiple frames submitted");
    frames.push(new Uint8ClampedArray(rgba));
  }
  const imports = Object.fromEntries(
    compiled.imports.map((name) => [name, { present }]),
  );
  const instance = instantiate((name) => compiled.modules[name], imports);
  const game = instance.game ?? instance["jungle:arena/game@1.0.0"];
  return {
    info: () => game.info(),
    init: (config) => game.init(config),
    move: (state, legal, budget) => game.chooseMove(state, legal, budget),
    frame: (state, flipped, progress) => {
      frames = [];
      game.render(state, flipped, progress);
      if (frames.length !== 1) throw Error("Missing frame");
      return frames[0];
    },
  };
}
