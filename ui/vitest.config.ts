import preact from "@preact/preset-vite";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";
import { i18nAlias } from "./vite.config.ts";

export default defineConfig({
  plugins: [preact()],
  resolve: {
    alias: {
      ...i18nAlias,
      // Node would take lucide's CommonJS build, which requires the real `react`: its ES module
      // build goes through Vite and gets aliased to preact/compat.
      "lucide-react": fileURLToPath(
        new URL("./node_modules/lucide-react/dist/esm/lucide-react.mjs", import.meta.url),
      ),
    },
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    // zustand and lucide-react import `react`: handled by Vite, not left to Node, it is aliased to preact/compat.
    server: { deps: { inline: ["zustand", "lucide-react"] } },
  },
});
