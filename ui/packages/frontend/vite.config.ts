import { fileURLToPath, URL } from "node:url";
import { paraglideVitePlugin } from "@inlang/paraglide-js";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";
import gzip from "rollup-plugin-gzip";
import { defineConfig } from "vite";
export default defineConfig({
  plugins: [
    paraglideVitePlugin({
      project: "./project.inlang",
      outdir: "./src/paraglide",
      emitTsDeclarations: true,
    }),
    svelte(),
    tailwindcss(),
    gzip(),
  ],
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  base: process.env.VITE_BASE_PATH || "/ui/",
  define: {
    APP_VERSION: JSON.stringify(process.env.VITE_APP_VERSION ?? "dev"),
  },
  server: {
    proxy: {
      "/api": "http://localhost:3001",
      "/events": "http://localhost:3001",
    },
  },
});
