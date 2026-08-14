import css from "./styles.css?raw";

describe("motion accessibility", () => {
  it("disables transitions and animations for reduced-motion users", () => {
    expect(css).toContain("@media (prefers-reduced-motion: reduce)");
    expect(css).toContain("animation-duration: 0.001ms !important");
    expect(css).toContain("transition-duration: 0.001ms !important");
  });

  it("keeps terrain, pieces, and visual feedback in separate stacking layers", () => {
    expect(css).toMatch(/\.square-layer\s*\{[^}]*z-index:\s*1/s);
    expect(css).toMatch(/\.piece-layer\s*\{[^}]*z-index:\s*2/s);
    expect(css).toMatch(/\.feedback-layer\s*\{[^}]*z-index:\s*3/s);
    expect(css).toMatch(/\.feedback-layer\s*\{[^}]*pointer-events:\s*none/s);
    expect(css).toMatch(/\.feedback-square--capture \.move-marker\s*\{/s);
  });
});
