/**
 * API configuration utility
 * Handles API base URL configuration based on environment variables
 */

// Get the API base URL from environment
// If not set, use relative URLs (same host as the UI)
export const API_BASE_URL = import.meta.env.VITE_API_BASE_URL || "";

// Check if we're in mock mode
export const isMockMode = import.meta.env.VITE_MOCK_MODE === "true";

/**
 * Creates fetch options with default settings
 * @param options - Additional fetch options
 * @returns Fetch options with defaults applied
 */
export function createFetchOptions(options: RequestInit = {}): RequestInit {
  return {
    headers: {
      "Content-Type": "application/json",
      ...options.headers,
    },
    ...options,
  };
}

/**
 * Wrapper for fetch that automatically handles API URL construction
 * @param endpoint - The API endpoint path
 * @param options - Fetch options
 * @returns Promise<Response>
 */
export async function apiFetch(
  endpoint: string,
  options: RequestInit = {}
): Promise<Response> {
  const url = `${API_BASE_URL}/${endpoint
    .replace(/^\//, "")
    .replace(/\/$/, "")}`;
  const fetchOptions = createFetchOptions(options);

  if (isMockMode) {
    console.log(`🔧 Mock API call: ${options.method || "GET"} ${url}`);
  }

  return fetch(url, fetchOptions);
}

/**
 * Wrapper for fetch that returns JSON data
 * @param endpoint - The API endpoint path
 * @param options - Fetch options
 * @returns Promise<T> - Parsed JSON response
 */
export async function apiJsonFetch<T = unknown>(
  endpoint: string,
  options: RequestInit = {}
): Promise<T> {
  const response = await apiFetch(endpoint, options);

  if (!response.ok) {
    throw new Error(`API error: ${response.status} ${response.statusText}`);
  }

  return response.json();
}

// Log current configuration in development
if (import.meta.env.DEV) {
  console.log("🔧 API Configuration:", {
    apiBaseUrl: API_BASE_URL || "(relative)",
    isMockMode,
    sampleUrl: `${API_BASE_URL}/temperature`, // Example endpoint
  });
}
