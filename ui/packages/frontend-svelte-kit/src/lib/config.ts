// In SvelteKit, we use $env modules for environment variables
import { PUBLIC_BASE_PATH } from '$env/static/public';
import { base } from '$app/paths';

// Get base path from environment variable or use SvelteKit's base
export const basePath = PUBLIC_BASE_PATH || base || '/';

export const basePathWithoutTrailingSlash = basePath.replace(/\/$/, '');
