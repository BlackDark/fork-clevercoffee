export const SERVER_BASE_URL = import.meta.env.VITE_SERVER_ENDPOINT || "";
export const API_BASE_URL = `${SERVER_BASE_URL}${import.meta.env.VITE_API_PATH || "/api"}`;
export const isMockMode = import.meta.env.VITE_MOCK_MODE === "true";
export function createFetchOptions(options: RequestInit = {}): RequestInit {
  const headers = new Headers(options.headers);
  if (
    options.body &&
    !(options.body instanceof FormData) &&
    !headers.has("Content-Type")
  )
    headers.set("Content-Type", "application/json");
  return { ...options, headers };
}
export function apiFetch(
  endpoint: string,
  options: RequestInit = {},
): Promise<Response> {
  return fetch(
    `${API_BASE_URL}/${endpoint.replace(/^\//, "")}`,
    createFetchOptions(options),
  );
}
export async function apiJsonFetch<T = unknown>(
  endpoint: string,
  options: RequestInit = {},
  timeoutMs = 10000,
): Promise<T> {
  const response = await apiFetch(endpoint, {
    ...options,
    signal: AbortSignal.any([
      ...(options.signal ? [options.signal] : []),
      AbortSignal.timeout(timeoutMs),
    ]),
  });
  const data: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const message =
      data && typeof data === "object" && "message" in data
        ? String(data.message)
        : data && typeof data === "object" && "error" in data
          ? String(data.error)
          : response.statusText;
    throw new Error(`HTTP ${response.status}: ${message}`);
  }
  return data as T;
}
