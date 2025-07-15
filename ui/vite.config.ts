import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";
import tailwindcss from "@tailwindcss/vite";
import type { UserConfig } from "vite";

// Get base path from environment variable or default to root
const basePath = process.env.VITE_BASE_PATH || "/";

// Valid UI routes that should return 200 status (without base path)
const validUIRoutes = ["/", "/settings", "/system", "/about"];

// Custom plugin to simulate 404 status codes in development
function custom404Plugin() {
  return {
    name: "custom-404-handler",
    // @ts-expect-error - Vite server types are complex, using simplified interface
    configureServer(server) {
      // @ts-expect-error - Middleware types are complex, using simplified interface
      server.middlewares.use((req, res, next) => {
        const url = req.url || "";

        // Remove base path from URL for route checking
        const normalizedUrl =
          basePath !== "/" && url.startsWith(basePath)
            ? url.slice(basePath.length - 1)
            : url;

        // Skip for assets, HMR, and API calls
        if (
          url.startsWith("/@") ||
          url.startsWith("/node_modules") ||
          url.includes(".") ||
          url.startsWith("/src/") ||
          url.startsWith("/__vite") ||
          url.startsWith("/api/") ||
          url.startsWith("/parameters") ||
          url.startsWith("/temperatures") ||
          url.startsWith("/history") ||
          url.startsWith("/livedata")
        ) {
          return next();
        }

        // Check if it's a valid UI route (using normalized URL)
        const isValidRoute = validUIRoutes.some((route) => {
          if (route === "/")
            return normalizedUrl === "/" || normalizedUrl === "";
          return normalizedUrl.startsWith(route);
        });

        // For invalid routes, log the 404 and add headers
        if (!isValidRoute && normalizedUrl !== "/" && normalizedUrl !== "") {
          console.log(
            `🔴 DEV 404: ${url} (normalized: ${normalizedUrl}) - would return HTTP 404 in production`
          );

          // Add custom headers to indicate this would be a 404
          res.setHeader("X-Dev-Status", "404");
          res.setHeader("X-Dev-Route-Type", "invalid");
        } else if (isValidRoute) {
          console.log(`✅ DEV 200: ${url} (normalized: ${normalizedUrl})`);
          res.setHeader("X-Dev-Status", "200");
          res.setHeader("X-Dev-Route-Type", "valid");
        }

        next();
      });
    },
  };
}

// https://vite.dev/config/
export default defineConfig(() => {
  const config: UserConfig = {
    plugins: [react(), tailwindcss(), custom404Plugin()],
    resolve: {
      alias: {
        "@": path.resolve(__dirname, "./src"),
      },
    },
    base: basePath,
  };

  return config;
});
