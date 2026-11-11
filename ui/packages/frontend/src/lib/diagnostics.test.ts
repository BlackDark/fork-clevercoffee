import { describe, expect, it } from "vitest";
import {
  formatKiB,
  formatMicros,
  formatStack,
  formatUptime,
  saveResultName,
} from "./diagnostics";

describe("diagnostics formatting", () => {
  it("formats uptime with days only when needed", () => {
    expect(formatUptime(0)).toBe("00:00:00");
    expect(formatUptime(3723000)).toBe("01:02:03");
    expect(formatUptime(90061000)).toBe("1d 01:01:01");
  });

  it("formats heap in whole KiB", () => {
    expect(formatKiB(68704)).toBe("67 KiB");
  });

  it("shows an unmeasured stack as such", () => {
    expect(formatStack(0)).toBe("not measured");
    expect(formatStack(19368)).toBe("19 KiB");
  });

  it("switches micros to milliseconds at 1 ms", () => {
    expect(formatMicros(850)).toBe("850 µs");
    expect(formatMicros(71000)).toBe("71.0 ms");
  });

  it("names save results and falls back for unknown codes", () => {
    expect(saveResultName(1)).toBe("saved");
    expect(saveResultName(9)).toBe("unknown");
  });
});
