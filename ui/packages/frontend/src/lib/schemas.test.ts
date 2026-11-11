import { describe, expect, it } from "vitest";
import {
  HistoryDataSchema,
  MetricsSchema,
  PidResponseSchema,
  SteamResponseSchema,
  TemperatureDataSchema,
} from "./schemas";

describe("API Schemas", () => {
  describe("MetricsSchema", () => {
    it("accepts the /api/metrics payload", () => {
      const parsed = MetricsSchema.parse({
        firmwareVersion: "2.2.0",
        resetReason: 1,
        uptime: 1000,
        freeHeap: 1,
        minFreeHeap: 1,
        minFreeStack: 1,
        lowHeapRefusals: 0,
        loopGapMaxUs: 1,
        loopTickMaxUs: 1,
        loopLateTicks: 0,
        tsicGoodFrames: 0,
        tsicBadFrames: 0,
        tsicLastGoodMs: 0,
        ds18b20ReadErrors: 0,
        settingsSaves: 0,
        settingsSaveFailures: 0,
        settingsLastSave: 0,
        logBytes: 0,
        wifiConnected: false,
        wifiRssi: 0,
        wifiReconnects: 0,
        mqttConnected: false,
      });
      expect(parsed.firmwareVersion).toBe("2.2.0");
    });

    it("rejects a missing counter", () => {
      expect(() => MetricsSchema.parse({ firmwareVersion: "x" })).toThrow();
    });
  });

  describe("TemperatureDataSchema", () => {
    it("parses valid temperature data", () => {
      const data = {
        currentTemp: 93.5,
        targetTemp: 93.0,
        heaterPower: 45.2,
      };
      const result = TemperatureDataSchema.parse(data);
      expect(result.currentTemp).toBe(93.5);
      expect(result.targetTemp).toBe(93.0);
      expect(result.heaterPower).toBe(45.2);
    });

    it("rejects invalid temperature data", () => {
      const data = { currentTemp: "hot" };
      expect(() => TemperatureDataSchema.parse(data)).toThrow();
    });
  });

  describe("HistoryDataSchema", () => {
    it("parses valid history data", () => {
      const data = {
        currentTemps: [93.0, 93.2, 93.5],
        targetTemps: [93.0, 93.0, 93.0],
        heaterPowers: [40, 45, 50],
      };
      const result = HistoryDataSchema.parse(data);
      expect(result.currentTemps).toHaveLength(3);
    });
  });

  describe("SteamResponseSchema", () => {
    it("parses valid steam response", () => {
      const data = { success: true, steamMode: true };
      const result = SteamResponseSchema.parse(data);
      expect(result.success).toBe(true);
      expect(result.steamMode).toBe(true);
    });
  });

  describe("PidResponseSchema", () => {
    it("parses valid PID response", () => {
      const data = { success: true, pidEnabled: false };
      const result = PidResponseSchema.parse(data);
      expect(result.success).toBe(true);
      expect(result.pidEnabled).toBe(false);
    });
  });
});

describe("parameter wire format", () => {
  it("accepts omitted or null bounds on string and enum parameters", async () => {
    const { ServerParametersSchema } = await import("./schemas");
    expect(
      ServerParametersSchema.parse([
        { name: "mqtt.password", type: 4, value: "secret" },
        { name: "display.template", type: 5, value: 0, min: null, max: null },
      ]),
    ).toHaveLength(2);
  });
});
