import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";
import tailwindcss from "@tailwindcss/vite";
import type { UserConfig } from "vite";

import gzipPlugin from "rollup-plugin-gzip";

// Get base path from environment variable or default to root
const basePath = process.env.VITE_BASE_PATH || "/";

// https://vite.dev/config/
export default defineConfig(() => {
  const config: UserConfig = {
    plugins: [react(), tailwindcss(), gzipPlugin()],
    resolve: {
      alias: {
        "@": path.resolve(__dirname, "./src"),
      },
    },
    base: basePath,
    build: {
      chunkSizeWarningLimit: 1000, // Increase chunk size warning limit to 1000 KB
    },
  };

  return config;
});
