import preact from "@preact/preset-vite";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

// Ported Sowel files import `react-i18next` and `i18next`: both are mini-Sowel's small `t()`
// (R9.4 of spec 014), so those files stay unchanged. Vitest reads the same aliases.
export const i18nAlias = {
  "react-i18next": fileURLToPath(new URL("./src/i18n/react-i18next.ts", import.meta.url)),
  i18next: fileURLToPath(new URL("./src/i18n/react-i18next.ts", import.meta.url)),
};

// The host program (`cargo run -p mini-sowel-host -- --sim ... --serve`) answers on 127.0.0.1:8080.
// `changeOrigin` makes the `Host` header the one its check accepts; the `Origin` of the page,
// `http://localhost:5173`, is the one its `/ws` accepts (spec 013).
const HOST = "http://127.0.0.1:8080";

// Preact with preact/compat: components written for React (Sowel's) work unchanged.
export default defineConfig({
  plugins: [preact(), tailwindcss()],
  resolve: { alias: i18nAlias },
  build: { target: "es2022" },
  server: {
    proxy: {
      "/api": { target: HOST, changeOrigin: true },
      "/ws": { target: HOST, changeOrigin: true, ws: true },
    },
  },
});
