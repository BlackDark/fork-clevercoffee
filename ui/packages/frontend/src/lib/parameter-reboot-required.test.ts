import { describe, expect, it } from "vitest";
import {
  getRebootParameterLabel,
  parameterRequiresReboot,
} from "./parameter-reboot-required";

describe("parameter-reboot-required", () => {
  it("flags display and hardware params that need reboot", () => {
    expect(parameterRequiresReboot("display.template")).toBe(true);
    expect(parameterRequiresReboot("hardware.sensors.scale.samples")).toBe(true);
    expect(parameterRequiresReboot("brew.setpoint")).toBe(false);
  });

  it("maps reboot param names to labels", () => {
    expect(getRebootParameterLabel("display.template")).toBe("Display Template");
  });
});
