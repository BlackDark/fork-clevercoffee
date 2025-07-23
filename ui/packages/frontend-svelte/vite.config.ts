import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import path from "path";
import type { UserConfig } from "vite";
import gzipPlugin from "rollup-plugin-gzip";

// Get base path from environment variable or default to root
const basePath = process.env.VITE_BASE_PATH || "/";

// https://vite.dev/config/
export default defineConfig(() => {
  const config: UserConfig = {
    plugins: [svelte(), tailwindcss(), gzipPlugin()],
    resolve: {
      alias: {
        $lib: path.resolve("./src/lib"),
        "@": path.resolve(__dirname, "./src"),
      },
    },
    base: basePath,
  };

  return config;
});
