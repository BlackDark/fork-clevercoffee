// Get base path from environment variable or use Vite's base
export const basePath =
  import.meta.env.VITE_BASE_PATH || import.meta.env.BASE_URL || "/";

export const basePathWithoutTrailingSlash = basePath.replace(/\/$/, "");
