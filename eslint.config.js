import eslint from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "generated", "release", "engine/target", "public/generated", "coverage", "playwright-report", "test-results"] },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["scripts/**/*.mjs", "conformance/**/*.mjs", "eslint.config.js"],
    languageOptions: {
      ecmaVersion: 2023,
      globals: { ...globals.node, ...globals.browser, ...globals.worker },
    },
  },
  {
    files: ["src/**/*.{ts,tsx}", "vite.config.ts", "playwright.config.ts"],
    languageOptions: {
      ecmaVersion: 2023,
      globals: { ...globals.browser, ...globals.worker, ...globals.node },
    },
    rules: {
      "@typescript-eslint/no-explicit-any": "off",
      "@typescript-eslint/no-unused-vars": ["error", { "argsIgnorePattern": "^_" }]
    },
  },
);
