import { describe, expect, it } from "vitest";
import { parseToggleResponse } from "./machine-toggle-result";

/**
 * A real `Response` carrying a JSON body.
 *
 * A real one rather than a stub cast to `Response`, because `pnpm build` runs
 * `tsc -b` over this file and a hand-rolled object does not satisfy the type.
 */
function jsonResponse(body: unknown, ok = true): Response {
  return new Response(JSON.stringify(body), { status: ok ? 200 : 500 });
}

describe("parseToggleResponse", () => {
  it("reports the state the device says it is now", () => {
    // The case the report was about: the device answers with the value it
    // actually reached, and the caller writes *that* into its own state rather
    // than assuming the toggle flipped.
    void parseToggleResponse(
      jsonResponse({ success: true, pidEnabled: false }),
      "pidEnabled",
    ).then((r) => {
      expect(r.success).toBe(true);
      expect(r.value).toBe(false);
    });
  });

  it("carries no value when the body does not have the key", () => {
    // A device that did not say must leave the caller to refetch rather than
    // guess — guessing is how the switch ended up showing the old value.
    void parseToggleResponse(
      jsonResponse({ success: true }),
      "pidEnabled",
    ).then((r) => {
      expect(r.success).toBe(true);
      expect(r.value).toBeUndefined();
    });
  });

  it("survives a body that is not JSON at all", () => {
    // An HTML error page from something in front of the device is the realistic
    // version of this, and it must not throw out of the toggle.
    const response = new Response("<html>502</html>", { status: 200 });
    void parseToggleResponse(response, "pidEnabled").then((r) => {
      expect(r.success).toBe(true);
      expect(r.value).toBeUndefined();
    });
  });

  it("accepts a numeric flag as a boolean", () => {
    void parseToggleResponse(
      jsonResponse({ success: true, backflushOn: 0 }),
      "backflushOn",
    ).then((r) => {
      expect(r.value).toBe(false);
    });
  });

  it("reports the error body on a failure", () => {
    void parseToggleResponse(
      jsonResponse({ error: "not keeping up" }, false),
      "pidEnabled",
    ).then((r) => {
      expect(r.success).toBe(false);
      expect(r.error).toBe("not keeping up");
    });
  });
});
