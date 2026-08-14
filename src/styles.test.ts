import css from "./styles.css?raw";

describe("motion accessibility", () => {
  it("disables transitions and animations for reduced-motion users", () => {
    expect(css).toContain("@media (prefers-reduced-motion: reduce)");
    expect(css).toContain("animation-duration: 0.001ms !important");
    expect(css).toContain("transition-duration: 0.001ms !important");
  });
});
