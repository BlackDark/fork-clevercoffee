import { describe, expect, it } from "vitest";
import { languageOptions } from "./language";

describe("language options", () => {
  it("names every locale", () => {
    for (const option of languageOptions) {
      expect(option.label).not.toBe(option.value);
    }
  });
});
