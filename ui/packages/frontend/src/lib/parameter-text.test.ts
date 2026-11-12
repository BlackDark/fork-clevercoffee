import { describe, expect, it } from "vitest";
import { defaultParametersList } from "./parameter-metadata";
import { getOptionLabel, getParameterLabel } from "./parameter-text";

describe("parameter text", () => {
  it("has a label for every known parameter", () => {
    for (const { name } of defaultParametersList) {
      expect(getParameterLabel(name), name).not.toBe(name);
    }
  });

  it("has a text for every enum option", () => {
    for (const param of defaultParametersList) {
      for (const option of param.options ?? []) {
        expect(
          getOptionLabel(param.name, option.value) ?? option.label,
          param.name,
        ).toBeTruthy();
      }
    }
  });
});
