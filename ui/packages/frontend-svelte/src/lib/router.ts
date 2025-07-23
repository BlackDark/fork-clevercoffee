import { writable, derived } from "svelte/store";
import { basePath } from "./config";

// Store for the current path
export const currentPath = writable<string>(
  typeof window !== "undefined" ? window.location.pathname : "/"
);

// Store for route parameters
export const routeParams = writable<Record<string, string>>({});

// Initialize the router
export function initRouter() {
  if (typeof window === "undefined") return;

  // Update path on navigation
  const updatePath = () => {
    currentPath.set(window.location.pathname);
    updateRouteParams();
  };

  // Handle navigation events
  window.addEventListener("popstate", updatePath);

  // Handle clicks on anchor tags
  document.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    const anchor = target.closest("a");

    if (
      anchor &&
      anchor.href &&
      anchor.href.startsWith(window.location.origin) &&
      !anchor.hasAttribute("target") &&
      !anchor.hasAttribute("download") &&
      !(event.ctrlKey || event.metaKey || event.shiftKey || event.altKey)
    ) {
      event.preventDefault();
      const url = new URL(anchor.href);

      // Only handle paths within our app
      if (url.pathname.startsWith(basePath)) {
        window.history.pushState({}, "", url.pathname);
        updatePath();
      } else {
        // External link or different base path, let the browser handle it
        window.location.href = anchor.href;
      }
    }
  });

  // Initial path update
  updatePath();
}

// Update route parameters based on current path and route pattern
function updateRouteParams() {
  const path = window.location.pathname;

  // Extract parameters from path
  const params: Record<string, string> = {};

  // Normalize path by removing basePath
  const normalizedPath = path.startsWith(basePath)
    ? path.substring(basePath.length) || "/"
    : path;

  // Example: /config/behavior -> { filter: 'behavior' }
  if (normalizedPath.startsWith("/config/")) {
    const filter = normalizedPath.substring("/config/".length);
    if (filter) params.filter = filter;
  }

  routeParams.set(params);
}

// Helper to check if a path matches the current path
export function matchPath(path: string): boolean {
  if (typeof window === "undefined") return false;

  const currentPathValue = window.location.pathname;

  // Normalize current path by removing basePath
  const normalizedCurrentPath = currentPathValue.startsWith(basePath)
    ? currentPathValue.substring(basePath.length) || "/"
    : currentPathValue;

  if (path === "/") {
    return normalizedCurrentPath === "/" || normalizedCurrentPath === "";
  }

  return normalizedCurrentPath.startsWith(path);
}

// Get the current component to render based on the path
export const currentComponent = derived(currentPath, ($currentPath) => {
  // Remove the base path from the current path
  const path = $currentPath.startsWith(basePath)
    ? $currentPath.substring(basePath.length) || "/"
    : $currentPath;

  // Simple routing logic
  if (path === "/" || path === "") return "home";
  if (path.startsWith("/config/")) return "config";
  if (path === "/system") return "system";
  if (path === "/about") return "about";

  // Default to 404 for unknown routes
  return "notFound";
});
