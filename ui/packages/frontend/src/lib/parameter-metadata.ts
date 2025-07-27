// Parameter metadata map for efficient lookups
// This replaces the old array-based parameter definitions

import type {
  ServerParameter,
  Parameter,
  ParameterTemplate,
} from "./parameter-types";
import { ParameterTypes } from "./parameter-types";

// Core parameter metadata - everything needed for UI display and validation
export const defaultParametersList: Array<ParameterTemplate> = [
  {
    name: "pid.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
  },
  {
    name: "pid.use_ponm",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
  },
  {
    name: "pid.ema_factor",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 1,
    defaultValue: 0.6,
  },
  {
    name: "pid.regular.kp",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 999,
    defaultValue: 50,
  },
  {
    name: "pid.regular.tn",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 999,
    defaultValue: 2,
  },
  {
    name: "pid.regular.tv",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 999,
    defaultValue: 0.1,
  },
  {
    name: "pid.regular.i_max",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 999,
    defaultValue: 192,
  },
  {
    name: "pid.steam.kp",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 999,
    defaultValue: 150,
  },

  // Temperature Control
  {
    name: "TEMP",
    type: ParameterTypes.DOUBLE,
    min: -50,
    max: 200,
    defaultValue: 0,
  },
  {
    name: "brew.setpoint",
    type: ParameterTypes.DOUBLE,
    min: 20,
    max: 110,
    defaultValue: 95,
  },
  {
    name: "brew.temp_offset",
    type: ParameterTypes.DOUBLE,
    min: -25,
    max: 25,
    defaultValue: 0,
  },
  {
    name: "steam.setpoint",
    type: ParameterTypes.DOUBLE,
    min: 100,
    max: 170,
    defaultValue: 135,
  },

  // Brew PID Parameters
  {
    name: "pid.bd.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
    requiredParameters: { "pid.enabled": true },
  },
  {
    name: "brew.pid_delay",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 999,
    defaultValue: 10,
    requiredParameters: { "pid.bd.enabled": true },
  },
  {
    name: "pid.bd.kp",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 999,
    defaultValue: 0,
    requiredParameters: { "pid.bd.enabled": true },
  },
  {
    name: "pid.bd.tn",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 999,
    defaultValue: 0,
    requiredParameters: { "pid.bd.enabled": true },
  },
  {
    name: "pid.bd.tv",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 999,
    defaultValue: 0,
    requiredParameters: { "pid.bd.enabled": true },
  },

  // Brew Control
  {
    name: "brew.mode",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "Manual" },
      { value: 1, label: "Automatic" },
    ],
    requiredParameters: { "hardware.switches.brew.enabled": true },
  },
  {
    name: "brew.by_time",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
    requiredParameters: { "brew.mode": 1 },
  },
  {
    name: "brew.target_time",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 999,
    defaultValue: 25,
    requiredParameters: { "brew.by_time": true },
  },
  {
    name: "brew.by_weight",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
    requiredParameters: { "brew.mode": 1 },
  },
  {
    name: "brew.target_weight",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 999,
    defaultValue: 30,
    requiredParameters: { "brew.by_weight": true },
  },
  {
    name: "brew.pre_infusion.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
    requiredParameters: { "brew.mode": 1 },
  },
  {
    name: "brew.pre_infusion.time",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 999,
    defaultValue: 2,
    requiredParameters: { "brew.pre_infusion.enabled": true },
  },
  {
    name: "brew.pre_infusion.pause",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 999,
    defaultValue: 5,
    requiredParameters: { "brew.pre_infusion.enabled": true },
  },

  // Maintenance Parameters
  {
    name: "backflush.cycles",
    type: ParameterTypes.UINT8,
    min: 1,
    max: 20,
    defaultValue: 5,
  },
  {
    name: "backflush.fill_time",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 60,
    defaultValue: 3,
  },
  {
    name: "backflush.flush_time",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 60,
    defaultValue: 6,
  },

  // Standby Parameters
  {
    name: "standby.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
  },
  {
    name: "standby.time",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 999,
    defaultValue: 60,
    requiredParameters: { "standby.enabled": true },
  },

  // Scale Parameters
  {
    name: "hardware.sensors.scale.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
  },
  {
    name: "hardware.sensors.scale.type",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "2 load cells" },
      { value: 1, label: "1 load cell" },
    ],
    requiredParameters: { "hardware.sensors.scale.enabled": true },
  },
  {
    name: "hardware.sensors.scale.samples",
    type: ParameterTypes.UINT8,
    min: 1,
    max: 20,
    defaultValue: 5,
    requiredParameters: { "hardware.sensors.scale.enabled": true },
  },
  {
    name: "hardware.sensors.scale.known_weight",
    type: ParameterTypes.DOUBLE,
    min: 1,
    max: 5000,
    defaultValue: 200,
    requiredParameters: { "hardware.sensors.scale.enabled": true },
  },
  {
    name: "hardware.sensors.scale.calibration",
    type: ParameterTypes.DOUBLE,
    min: -10000,
    max: 10000,
    defaultValue: 1,
    requiredParameters: { "hardware.sensors.scale.enabled": true },
  },
  {
    name: "hardware.sensors.scale.calibration2",
    type: ParameterTypes.DOUBLE,
    min: -10000,
    max: 10000,
    defaultValue: 1,
    requiredParameters: { "hardware.sensors.scale.enabled": true },
  },

  // MQTT Parameters
  {
    name: "mqtt.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
  },
  {
    name: "mqtt.broker",
    type: ParameterTypes.STRING,
    min: 0,
    max: 64,
    defaultValue: "",
    requiredParameters: { "mqtt.enabled": true },
  },
  {
    name: "mqtt.port",
    type: ParameterTypes.INTEGER,
    min: 1,
    max: 65535,
    defaultValue: 1883,
    requiredParameters: { "mqtt.enabled": true },
  },
  {
    name: "mqtt.username",
    type: ParameterTypes.STRING,
    min: 0,
    max: 32,
    defaultValue: "",
    requiredParameters: { "mqtt.enabled": true },
  },
  {
    name: "mqtt.password",
    type: ParameterTypes.STRING,
    min: 0,
    max: 32,
    defaultValue: "",
    requiredParameters: { "mqtt.enabled": true },
  },
  {
    name: "mqtt.topic",
    type: ParameterTypes.STRING,
    min: 0,
    max: 64,
    defaultValue: "",
    requiredParameters: { "mqtt.enabled": true },
  },
  {
    name: "mqtt.hassio.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
    requiredParameters: { "mqtt.enabled": true },
  },
  {
    name: "mqtt.hassio.prefix",
    type: ParameterTypes.STRING,
    min: 0,
    max: 32,
    defaultValue: "",
    requiredParameters: { "mqtt.hassio.enabled": true },
  },

  // System Parameters
  {
    name: "system.hostname",
    type: ParameterTypes.STRING,
    min: 0,
    max: 32,
    defaultValue: "clevercoffee",
  },
  {
    name: "system.ota_password",
    type: ParameterTypes.STRING,
    min: 0,
    max: 32,
    defaultValue: "",
  },
  {
    name: "system.log_level",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 6,
    defaultValue: 2,
    options: [
      { value: 0, label: "TRACE" },
      { value: 1, label: "DEBUG" },
      { value: 2, label: "INFO" },
      { value: 3, label: "WARNING" },
      { value: 4, label: "ERROR" },
      { value: 5, label: "FATAL" },
      { value: 6, label: "SILENT" },
    ],
  },
  {
    name: "system.auth.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
  },
  {
    name: "system.auth.username",
    type: ParameterTypes.STRING,
    min: 0,
    max: 32,
    defaultValue: "",
    requiredParameters: { "system.auth.enabled": true },
  },
  {
    name: "system.auth.password",
    type: ParameterTypes.STRING,
    min: 0,
    max: 32,
    defaultValue: "",
    requiredParameters: { "system.auth.enabled": true },
  },
  {
    name: "system.timing_debug.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
    requiredParameters: { "system.log_level": 1 },
  },
  {
    name: "system.showdisplay.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
    requiredParameters: { "system.log_level": 1 },
  },

  // Hardware - OLED Parameters
  {
    name: "hardware.oled.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: true,
  },
  {
    name: "hardware.oled.type",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "SH1106" },
      { value: 1, label: "SSD1306" },
    ],
    requiredParameters: { "hardware.oled.enabled": true },
  },
  {
    name: "hardware.oled.address",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "0x3C" },
      { value: 1, label: "0x3D" },
    ],
    requiredParameters: { "hardware.oled.enabled": true },
  },

  // Hardware - Relay Parameters
  {
    name: "hardware.relays.heater.trigger_type",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "Low Trigger" },
      { value: 1, label: "High Trigger" },
    ],
  },
  {
    name: "hardware.relays.valve.trigger_type",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "Low Trigger" },
      { value: 1, label: "High Trigger" },
    ],
  },
  {
    name: "hardware.relays.pump.trigger_type",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "Low Trigger" },
      { value: 1, label: "High Trigger" },
    ],
  },

  // Hardware - Switch Parameters
  {
    name: "hardware.switches.brew.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
  },
  {
    name: "hardware.switches.brew.type",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "Momentary" },
      { value: 1, label: "Toggle" },
    ],
    requiredParameters: { "hardware.switches.brew.enabled": true },
  },
  {
    name: "hardware.switches.brew.mode",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "Normally Open" },
      { value: 1, label: "Normally Closed" },
    ],
    requiredParameters: { "hardware.switches.brew.enabled": true },
  },
  {
    name: "hardware.switches.steam.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
  },
  {
    name: "hardware.switches.steam.type",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "Momentary" },
      { value: 1, label: "Toggle" },
    ],
    requiredParameters: { "hardware.switches.steam.enabled": true },
  },
  {
    name: "hardware.switches.steam.mode",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "Normally Open" },
      { value: 1, label: "Normally Closed" },
    ],
    requiredParameters: { "hardware.switches.steam.enabled": true },
  },
  {
    name: "hardware.switches.power.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
  },
  {
    name: "hardware.switches.power.type",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "Momentary" },
      { value: 1, label: "Toggle" },
    ],
    requiredParameters: { "hardware.switches.power.enabled": true },
  },
  {
    name: "hardware.switches.power.mode",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "Normally Open" },
      { value: 1, label: "Normally Closed" },
    ],
    requiredParameters: { "hardware.switches.power.enabled": true },
  },
  {
    name: "hardware.switches.hot_water.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
  },
  {
    name: "hardware.switches.hot_water.type",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "Momentary" },
      { value: 1, label: "Toggle" },
    ],
    requiredParameters: { "hardware.switches.hot_water.enabled": true },
  },
  {
    name: "hardware.switches.hot_water.mode",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "Normally Open" },
      { value: 1, label: "Normally Closed" },
    ],
    requiredParameters: { "hardware.switches.hot_water.enabled": true },
  },

  // Hardware - LED Parameters
  {
    name: "hardware.leds.status.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
  },
  {
    name: "hardware.leds.status.inverted",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
    requiredParameters: { "hardware.leds.status.enabled": true },
  },
  {
    name: "hardware.leds.brew.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
  },
  {
    name: "hardware.leds.brew.inverted",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
    requiredParameters: { "hardware.leds.brew.enabled": true },
  },
  {
    name: "hardware.leds.steam.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
  },
  {
    name: "hardware.leds.steam.inverted",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
    requiredParameters: { "hardware.leds.steam.enabled": true },
  },

  // Hardware - Sensor Parameters
  {
    name: "hardware.sensors.temperature.type",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "TSIC306" },
      { value: 1, label: "Dallas DS18B20" },
    ],
  },
  {
    name: "hardware.sensors.pressure.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
  },
  {
    name: "hardware.sensors.watertank.enabled",
    type: ParameterTypes.BOOL,
    min: 0,
    max: 1,
    defaultValue: false,
  },
  {
    name: "hardware.sensors.watertank.mode",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 1,
    defaultValue: 0,
    options: [
      { value: 0, label: "Normally Open" },
      { value: 1, label: "Normally Closed" },
    ],
    requiredParameters: { "hardware.sensors.watertank.enabled": true },
  },

  // Display Parameters
  {
    name: "display.template",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 4,
    defaultValue: 0,
    options: [
      { value: 0, label: "Standard" },
      { value: 1, label: "Minimal" },
      { value: 2, label: "Temp only" },
      { value: 3, label: "Scale" },
      { value: 4, label: "Upright" },
    ],
  },
  {
    name: "display.inverted",
    type: ParameterTypes.UINT8,
    min: 0,
    max: 1,
    defaultValue: 0,
  },
  {
    name: "display.language",
    type: ParameterTypes.ENUM,
    min: 0,
    max: 2,
    defaultValue: 0,
    options: [
      { value: 0, label: "Deutsch" },
      { value: 1, label: "English" },
      { value: 2, label: "Español" },
    ],
  },
  {
    name: "display.fullscreen_brew_timer",
    type: ParameterTypes.UINT8,
    min: 0,
    max: 1,
    defaultValue: 0,
  },
  {
    name: "display.fullscreen_manual_flush_timer",
    type: ParameterTypes.UINT8,
    min: 0,
    max: 1,
    defaultValue: 0,
  },
  {
    name: "display.fullscreen_hot_water_timer",
    type: ParameterTypes.UINT8,
    min: 0,
    max: 1,
    defaultValue: 0,
  },
  {
    name: "display.post_brew_timer_duration",
    type: ParameterTypes.DOUBLE,
    min: 0,
    max: 300,
    defaultValue: 5,
  },
  {
    name: "display.heating_logo",
    type: ParameterTypes.UINT8,
    min: 0,
    max: 1,
    defaultValue: 1,
  },
  {
    name: "display.pid_off_logo",
    type: ParameterTypes.UINT8,
    min: 0,
    max: 1,
    defaultValue: 1,
  },

  // System Parameters
  {
    name: "VERSION",
    type: ParameterTypes.STRING,
    min: 0,
    max: 100,
    defaultValue: "unknown",
  },
  {
    name: "STEAM_MODE",
    type: ParameterTypes.UINT8,
    min: 0,
    max: 1,
    defaultValue: 0,
  },
  {
    name: "BACKFLUSH_ON",
    type: ParameterTypes.UINT8,
    min: 0,
    max: 1,
    defaultValue: 0,
  },
  {
    name: "TARE_ON",
    type: ParameterTypes.UINT8,
    min: 0,
    max: 1,
    defaultValue: 0,
  },
  {
    name: "CALIBRATION_ON",
    type: ParameterTypes.UINT8,
    min: 0,
    max: 1,
    defaultValue: 0,
  },
];

const mappedParameters = new Map<string, Parameter>(
  defaultParametersList.map((p) => [p.name, { ...p, value: p.defaultValue }])
);

/**
 * Get parameter metadata by name
 */
export function getParameterMetadata(name: string): Parameter | undefined {
  return mappedParameters.get(name);
}

/**
 * Check if a parameter exists in metadata
 */
export function hasParameter(name: string): boolean {
  return mappedParameters.has(name);
}

export const mapServerParameterToParameter = (
  serverParameter: ServerParameter
): Parameter => {
  const metadata = mappedParameters.get(serverParameter.name);

  return {
    ...serverParameter,
    defaultValue: metadata?.defaultValue ?? serverParameter.value,
  };
};

/**
 * Get all parameter names that have metadata
 */
export function getAllParameterNames(): string[] {
  return Array.from(mappedParameters.keys());
}

/**
 * Create a parameter with default values from metadata
 */
export function createParameterWithDefaults(
  name: string
): Parameter | undefined {
  const metadata = mappedParameters.get(name);

  if (!metadata) {
    return undefined;
  }

  return {
    type: metadata.type,
    name,
    value: metadata.value,
    defaultValue: metadata.defaultValue,
    min: metadata.min,
    max: metadata.max,
    options: metadata.options,
    requiredParameters: metadata.requiredParameters,
  };
}

/**
 * Ensure all metadata parameters exist in server parameters list
 * This fills in missing parameters with default values
 */
export function ensureCompleteParameters(
  serverParameters: ServerParameter[]
): Parameter[] {
  const serverParamMap = new Map(serverParameters.map((p) => [p.name, p]));
  const result: Parameter[] = [];

  // Add all server parameters first
  for (const serverParam of serverParameters) {
    const mappedServerParam = mapServerParameterToParameter(serverParam);

    result.push({
      ...mappedParameters.get(serverParam.name),
      ...mappedServerParam,
    });
  }

  // Add missing parameters with defaults
  for (const [name] of mappedParameters) {
    if (!serverParamMap.has(name)) {
      const defaultParam = createParameterWithDefaults(name);
      if (defaultParam) {
        result.push(defaultParam);
      }
    }
  }

  return result;
}
