import { describe, expect, it } from "vitest";
import { encodeParameterChanges } from "./parameter-save";

const parameter = {
  name: "brew.setpoint",
  type: 2 as const,
  value: 93,
  defaultValue: 93,
  min: 0,
  max: 150,
};
describe("parameter saves", () => {
  it("sends only explicitly changed keys", () =>
    expect(
      encodeParameterChanges(
        [{ name: parameter.name, value: 94 }],
        [parameter],
      ).toString(),
    ).toBe("brew.setpoint=94"));
  it("rejects invalid numeric drafts even when their category is not visible", () => {
    for (const value of [NaN, Infinity, 151])
      expect(() =>
        encodeParameterChanges([{ name: parameter.name, value }], [parameter]),
      ).toThrow();
  });
  it("counts encoded bytes before sending to the firmware", () => {
    const p = {
      ...parameter,
      name: "mqtt.password",
      type: 4 as const,
      value: "",
    };
    expect(() =>
      encodeParameterChanges([{ name: p.name, value: "ä".repeat(200) }], [p]),
    ).toThrow("1024");
  });
  it("never invents writable parameters absent from the device", () =>
    expect(() =>
      encodeParameterChanges([{ name: "missing", value: true }], [parameter]),
    ).toThrow("Unknown device parameter"));
});
