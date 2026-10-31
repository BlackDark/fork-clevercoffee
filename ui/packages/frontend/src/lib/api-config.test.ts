import { afterEach, describe, expect, it, vi } from "vitest";
import { apiJsonFetch, createFetchOptions } from "./api-config";

afterEach(() => vi.unstubAllGlobals());
describe("API requests", () => {
  it("preserves supplied headers and lets the browser set multipart boundaries", () => {
    const options = createFetchOptions({
      headers: new Headers({ Authorization: "Basic test" }),
      body: new FormData(),
    });
    const headers = new Headers(options.headers);
    expect(headers.get("Authorization")).toBe("Basic test");
    expect(headers.has("Content-Type")).toBe(false);
  });
  it("surfaces device rejection messages", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ message: "Machine is brewing" }), {
          status: 409,
        }),
      ),
    );
    await expect(apiJsonFetch("/ota/firmware")).rejects.toThrow(
      "HTTP 409: Machine is brewing",
    );
  });
});
