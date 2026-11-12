import { describe, expect, it } from "vitest";
import { defaultParametersList } from "./parameter-metadata";
import { getParameterLabel } from "./parameter-text";

describe("parameter text", () => {
  it("has a label for every known parameter", () => {
    for (const { name } of defaultParametersList) {
      expect(getParameterLabel(name), name).not.toBe(name);
    }
  });
});
