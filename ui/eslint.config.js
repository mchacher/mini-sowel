import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";
import { defineConfig, globalIgnores } from "eslint/config";
import designClasses from "./eslint/design-classes.js";

export default defineConfig([
  globalIgnores(["dist"]),
  {
    files: ["**/*.{ts,tsx}"],
    extends: [js.configs.recommended, tseslint.configs.recommended],
    languageOptions: { ecmaVersion: 2022, globals: globals.browser },
  },
  {
    files: ["scripts/**/*.mjs", "eslint/**/*.js"],
    extends: [js.configs.recommended],
    languageOptions: { ecmaVersion: 2022, globals: globals.node },
  },
  {
    // Sowel's design rules (docs/design-system.md): the type scale, sentence case and the radius
    // scale, from Sowel's eslint/design-classes.js, copied unchanged.
    files: ["src/**/*.{ts,tsx}"],
    ignores: ["**/*.test.{ts,tsx}"],
    plugins: { sowel: designClasses },
    rules: {
      "sowel/type-scale": "error",
      "sowel/sentence-case": "error",
      "sowel/radius-scale": "error",
    },
  },
  {
    // Colours come from the tokens only; Lucide icons are imported by name, never all of them
    // (the box serves a first load under the budget of scripts/check-bundle-budget.mjs).
    files: ["src/**/*.{ts,tsx}"],
    ignores: ["**/*.test.{ts,tsx}"],
    rules: {
      "no-restricted-syntax": [
        "error",
        {
          selector: "Literal[value=/#[0-9a-fA-F]{3,8}\\b/]",
          message: "No raw colour: use a token (docs/design-system.md).",
        },
        {
          selector: "TemplateElement[value.raw=/#[0-9a-fA-F]{3,8}\\b/]",
          message: "No raw colour: use a token (docs/design-system.md).",
        },
        {
          selector: "ImportExpression[source.value='lucide-react']",
          message: "import('lucide-react') pulls every Lucide icon: import icons by name.",
        },
        {
          selector: "ExportAllDeclaration[source.value='lucide-react']",
          message: "export * from 'lucide-react' pulls every Lucide icon: re-export icons by name.",
        },
      ],
      "no-restricted-imports": [
        "error",
        {
          paths: [
            {
              name: "lucide-react",
              importNames: ["icons"],
              message: "This pulls every Lucide icon into the bundle: import icons by name.",
            },
            {
              name: "lucide-react/dynamic",
              message: "Dynamic Lucide icons pull every icon: import icons by name.",
            },
          ],
        },
      ],
    },
  },
]);
