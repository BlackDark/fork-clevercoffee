import { describe, expect, it } from "vitest";
import { defaultParametersList } from "./parameter-metadata";
import { getOptionLabel } from "./parameter-options";
import { getParameterLabel } from "./parameter-text";

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

  it("keeps OLED labels in the firmware's driver order", () => {
    const oled = defaultParametersList.find(
      (p) => p.name === "hardware.oled.type",
    );
    expect(oled?.options?.map((o) => o.label)).toEqual(["SSD1306", "SH1106"]);
  });
});
