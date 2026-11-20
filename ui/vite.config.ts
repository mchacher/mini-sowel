import preact from "@preact/preset-vite";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

// Preact with preact/compat: components written for React (Sowel's) work unchanged.
export default defineConfig({
  plugins: [preact(), tailwindcss()],
  build: { target: "es2022" },
});
