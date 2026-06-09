import { describe, expect, it } from "vitest";
import {
  HistoryDataSchema,
  PidResponseSchema,
  SteamResponseSchema,
  TemperatureDataSchema,
} from "./schemas";

describe("API Schemas", () => {
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
