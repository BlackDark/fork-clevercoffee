// Parameter metadata map for efficient lookups
// This replaces the old array-based parameter definitions

import type {
  ParameterMetadata,
  ServerParameter,
  Parameter,
} from "./parameter-types";
import { ParameterTypes } from "./parameter-types";

// Core parameter metadata - everything needed for UI display and validation
export const parameterMetadata = new Map<string, ParameterMetadata>([
  // PID Parameters
  [
    "pid.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "pid.use_ponm",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "pid.ema_factor",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 1,
      defaultValue: 0.6,
    },
  ],
  [
    "pid.regular.kp",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 999,
      defaultValue: 50,
    },
  ],
  [
    "pid.regular.tn",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 999,
      defaultValue: 2,
    },
  ],
  [
    "pid.regular.tv",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 999,
      defaultValue: 0.1,
    },
  ],
  [
    "pid.regular.i_max",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 999,
      defaultValue: 192,
    },
  ],
  [
    "pid.steam.kp",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 999,
      defaultValue: 150,
    },
  ],

  // Temperature Control
  [
    "TEMP",
    {
      type: ParameterTypes.DOUBLE,
      min: -50,
      max: 200,
      defaultValue: 0,
    },
  ],
  [
    "brew.setpoint",
    {
      type: ParameterTypes.DOUBLE,
      min: 20,
      max: 110,
      defaultValue: 95,
    },
  ],
  [
    "brew.temp_offset",
    {
      type: ParameterTypes.DOUBLE,
      min: -25,
      max: 25,
      defaultValue: 0,
    },
  ],
  [
    "steam.setpoint",
    {
      type: ParameterTypes.DOUBLE,
      min: 100,
      max: 170,
      defaultValue: 135,
    },
  ],

  // Brew PID Parameters
  [
    "pid.bd.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
      requiredParameters: { "pid.enabled": 1 },
    },
  ],
  [
    "brew.pid_delay",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 999,
      defaultValue: 10,
      requiredParameters: { "pid.bd.enabled": 1 },
    },
  ],
  [
    "pid.bd.kp",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 999,
      defaultValue: 0,
      requiredParameters: { "pid.bd.enabled": 1 },
    },
  ],
  [
    "pid.bd.tn",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 999,
      defaultValue: 0,
      requiredParameters: { "pid.bd.enabled": 1 },
    },
  ],
  [
    "pid.bd.tv",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 999,
      defaultValue: 0,
      requiredParameters: { "pid.bd.enabled": 1 },
    },
  ],

  // Brew Control
  [
    "brew.mode",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "Manual" },
        { value: 1, label: "Automatic" },
      ],
      requiredParameters: { "hardware.switches.brew.enabled": 1 },
    },
  ],
  [
    "brew.by_time",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
      requiredParameters: { "brew.mode": 1 },
    },
  ],
  [
    "brew.target_time",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 999,
      defaultValue: 25,
      requiredParameters: { "brew.by_time": 1 },
    },
  ],
  [
    "brew.by_weight",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
      requiredParameters: { "brew.mode": 1 },
    },
  ],
  [
    "brew.target_weight",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 999,
      defaultValue: 30,
      requiredParameters: { "brew.by_weight": 1 },
    },
  ],
  [
    "brew.pre_infusion.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
      requiredParameters: { "brew.mode": 1 },
    },
  ],
  [
    "brew.pre_infusion.time",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 999,
      defaultValue: 2,
      requiredParameters: { "brew.pre_infusion.enabled": 1 },
    },
  ],
  [
    "brew.pre_infusion.pause",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 999,
      defaultValue: 5,
      requiredParameters: { "brew.pre_infusion.enabled": 1 },
    },
  ],

  // Maintenance Parameters
  [
    "backflush.cycles",
    {
      type: ParameterTypes.UINT8,
      min: 1,
      max: 20,
      defaultValue: 5,
    },
  ],
  [
    "backflush.fill_time",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 60,
      defaultValue: 3,
    },
  ],
  [
    "backflush.flush_time",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 60,
      defaultValue: 6,
    },
  ],

  // Standby Parameters
  [
    "standby.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "standby.time",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 999,
      defaultValue: 60,
      requiredParameters: { "standby.enabled": 1 },
    },
  ],

  // Scale Parameters
  [
    "hardware.sensors.scale.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "hardware.sensors.scale.type",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "2 load cells" },
        { value: 1, label: "1 load cell" },
      ],
      requiredParameters: { "hardware.sensors.scale.enabled": 1 },
    },
  ],
  [
    "hardware.sensors.scale.samples",
    {
      type: ParameterTypes.UINT8,
      min: 1,
      max: 20,
      defaultValue: 5,
      requiredParameters: { "hardware.sensors.scale.enabled": 1 },
    },
  ],
  [
    "hardware.sensors.scale.known_weight",
    {
      type: ParameterTypes.DOUBLE,
      min: 1,
      max: 5000,
      defaultValue: 200,
      requiredParameters: { "hardware.sensors.scale.enabled": 1 },
    },
  ],
  [
    "hardware.sensors.scale.calibration",
    {
      type: ParameterTypes.DOUBLE,
      min: -10000,
      max: 10000,
      defaultValue: 1,
      requiredParameters: { "hardware.sensors.scale.enabled": 1 },
    },
  ],
  [
    "hardware.sensors.scale.calibration2",
    {
      type: ParameterTypes.DOUBLE,
      min: -10000,
      max: 10000,
      defaultValue: 1,
      requiredParameters: { "hardware.sensors.scale.enabled": 1 },
    },
  ],

  // MQTT Parameters
  [
    "mqtt.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "mqtt.broker",
    {
      type: ParameterTypes.STRING,
      min: 0,
      max: 64,
      defaultValue: "",
      requiredParameters: { "mqtt.enabled": 1 },
    },
  ],
  [
    "mqtt.port",
    {
      type: ParameterTypes.INTEGER,
      min: 1,
      max: 65535,
      defaultValue: 1883,
      requiredParameters: { "mqtt.enabled": 1 },
    },
  ],
  [
    "mqtt.username",
    {
      type: ParameterTypes.STRING,
      min: 0,
      max: 32,
      defaultValue: "",
      requiredParameters: { "mqtt.enabled": 1 },
    },
  ],
  [
    "mqtt.password",
    {
      type: ParameterTypes.STRING,
      min: 0,
      max: 32,
      defaultValue: "",
      requiredParameters: { "mqtt.enabled": 1 },
    },
  ],
  [
    "mqtt.topic",
    {
      type: ParameterTypes.STRING,
      min: 0,
      max: 64,
      defaultValue: "",
      requiredParameters: { "mqtt.enabled": 1 },
    },
  ],
  [
    "mqtt.hassio.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
      requiredParameters: { "mqtt.enabled": 1 },
    },
  ],
  [
    "mqtt.hassio.prefix",
    {
      type: ParameterTypes.STRING,
      min: 0,
      max: 32,
      defaultValue: "",
      requiredParameters: { "mqtt.hassio.enabled": 1 },
    },
  ],

  // System Parameters
  [
    "system.hostname",
    {
      type: ParameterTypes.STRING,
      min: 0,
      max: 32,
      defaultValue: "clevercoffee",
    },
  ],
  [
    "system.ota_password",
    {
      type: ParameterTypes.STRING,
      min: 0,
      max: 32,
      defaultValue: "",
    },
  ],
  [
    "system.log_level",
    {
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
  ],
  [
    "system.auth.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "system.auth.username",
    {
      type: ParameterTypes.STRING,
      min: 0,
      max: 32,
      defaultValue: "",
      requiredParameters: { "system.auth.enabled": 1 },
    },
  ],
  [
    "system.auth.password",
    {
      type: ParameterTypes.STRING,
      min: 0,
      max: 32,
      defaultValue: "",
      requiredParameters: { "system.auth.enabled": 1 },
    },
  ],
  [
    "system.timing_debug.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
      requiredParameters: { "system.log_level": 1 },
    },
  ],
  [
    "system.showdisplay.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
      requiredParameters: { "system.log_level": 1 },
    },
  ],

  // Hardware - OLED Parameters
  [
    "hardware.oled.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 1,
    },
  ],
  [
    "hardware.oled.type",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "SH1106" },
        { value: 1, label: "SSD1306" },
      ],
      requiredParameters: { "hardware.oled.enabled": 1 },
    },
  ],
  [
    "hardware.oled.address",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "0x3C" },
        { value: 1, label: "0x3D" },
      ],
      requiredParameters: { "hardware.oled.enabled": 1 },
    },
  ],

  // Hardware - Relay Parameters
  [
    "hardware.relays.heater.trigger_type",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "Low Trigger" },
        { value: 1, label: "High Trigger" },
      ],
    },
  ],
  [
    "hardware.relays.valve.trigger_type",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "Low Trigger" },
        { value: 1, label: "High Trigger" },
      ],
    },
  ],
  [
    "hardware.relays.pump.trigger_type",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "Low Trigger" },
        { value: 1, label: "High Trigger" },
      ],
    },
  ],

  // Hardware - Switch Parameters
  [
    "hardware.switches.brew.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "hardware.switches.brew.type",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "Momentary" },
        { value: 1, label: "Toggle" },
      ],
      requiredParameters: { "hardware.switches.brew.enabled": 1 },
    },
  ],
  [
    "hardware.switches.brew.mode",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "Normally Open" },
        { value: 1, label: "Normally Closed" },
      ],
      requiredParameters: { "hardware.switches.brew.enabled": 1 },
    },
  ],
  [
    "hardware.switches.steam.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "hardware.switches.steam.type",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "Momentary" },
        { value: 1, label: "Toggle" },
      ],
      requiredParameters: { "hardware.switches.steam.enabled": 1 },
    },
  ],
  [
    "hardware.switches.steam.mode",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "Normally Open" },
        { value: 1, label: "Normally Closed" },
      ],
      requiredParameters: { "hardware.switches.steam.enabled": 1 },
    },
  ],
  [
    "hardware.switches.power.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "hardware.switches.power.type",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "Momentary" },
        { value: 1, label: "Toggle" },
      ],
      requiredParameters: { "hardware.switches.power.enabled": 1 },
    },
  ],
  [
    "hardware.switches.power.mode",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "Normally Open" },
        { value: 1, label: "Normally Closed" },
      ],
      requiredParameters: { "hardware.switches.power.enabled": 1 },
    },
  ],
  [
    "hardware.switches.hot_water.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "hardware.switches.hot_water.type",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "Momentary" },
        { value: 1, label: "Toggle" },
      ],
      requiredParameters: { "hardware.switches.hot_water.enabled": 1 },
    },
  ],
  [
    "hardware.switches.hot_water.mode",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "Normally Open" },
        { value: 1, label: "Normally Closed" },
      ],
      requiredParameters: { "hardware.switches.hot_water.enabled": 1 },
    },
  ],

  // Hardware - LED Parameters
  [
    "hardware.leds.status.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "hardware.leds.status.inverted",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
      requiredParameters: { "hardware.leds.status.enabled": 1 },
    },
  ],
  [
    "hardware.leds.brew.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "hardware.leds.brew.inverted",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
      requiredParameters: { "hardware.leds.brew.enabled": 1 },
    },
  ],
  [
    "hardware.leds.steam.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "hardware.leds.steam.inverted",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
      requiredParameters: { "hardware.leds.steam.enabled": 1 },
    },
  ],

  // Hardware - Sensor Parameters
  [
    "hardware.sensors.temperature.type",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "TSIC306" },
        { value: 1, label: "Dallas DS18B20" },
      ],
    },
  ],
  [
    "hardware.sensors.pressure.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "hardware.sensors.watertank.enabled",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "hardware.sensors.watertank.mode",
    {
      type: ParameterTypes.ENUM,
      min: 0,
      max: 1,
      defaultValue: 0,
      options: [
        { value: 0, label: "Normally Open" },
        { value: 1, label: "Normally Closed" },
      ],
      requiredParameters: { "hardware.sensors.watertank.enabled": 1 },
    },
  ],

  // Display Parameters
  [
    "display.template",
    {
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
  ],
  [
    "display.inverted",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "display.language",
    {
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
  ],
  [
    "display.fullscreen_brew_timer",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "display.fullscreen_manual_flush_timer",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "display.fullscreen_hot_water_timer",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "display.post_brew_timer_duration",
    {
      type: ParameterTypes.DOUBLE,
      min: 0,
      max: 300,
      defaultValue: 5,
    },
  ],
  [
    "display.heating_logo",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 1,
    },
  ],
  [
    "display.pid_off_logo",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 1,
    },
  ],

  // System Parameters
  [
    "VERSION",
    {
      type: ParameterTypes.STRING,
      min: 0,
      max: 100,
      defaultValue: "unknown",
    },
  ],
  [
    "STEAM_MODE",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "BACKFLUSH_ON",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "TARE_ON",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
  [
    "CALIBRATION_ON",
    {
      type: ParameterTypes.UINT8,
      min: 0,
      max: 1,
      defaultValue: 0,
    },
  ],
]);

/**
 * Get parameter metadata by name
 */
export function getParameterMetadata(
  name: string
): ParameterMetadata | undefined {
  return parameterMetadata.get(name);
}

/**
 * Check if a parameter exists in metadata
 */
export function hasParameterMetadata(name: string): boolean {
  return parameterMetadata.has(name);
}

/**
 * Merge server parameter with metadata to create complete UI parameter
 */
export function mergeParameterWithMetadata(
  serverParam: ServerParameter
): Parameter {
  const metadata = parameterMetadata.get(serverParam.name);
  if (!metadata) {
    // Return server parameter as-is if no metadata found
    return serverParam;
  }

  return {
    ...serverParam,
    requiredParameters: metadata.requiredParameters,
  };
}

/**
 * Merge array of server parameters with metadata
 */
export function mergeParametersWithMetadata(
  serverParameters: ServerParameter[]
): Parameter[] {
  return serverParameters.map(mergeParameterWithMetadata);
}

/**
 * Get all parameter names that have metadata
 */
export function getAllParameterNames(): string[] {
  return Array.from(parameterMetadata.keys());
}

/**
 * Create a parameter with default values from metadata
 */
export function createParameterWithDefaults(
  name: string
): Parameter | undefined {
  const metadata = parameterMetadata.get(name);
  if (!metadata) {
    return undefined;
  }

  return {
    type: metadata.type,
    name,
    value: metadata.defaultValue,
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
    result.push(mergeParameterWithMetadata(serverParam));
  }

  // Add missing parameters with defaults
  for (const [name] of parameterMetadata) {
    if (!serverParamMap.has(name)) {
      const defaultParam = createParameterWithDefaults(name);
      if (defaultParam) {
        result.push(defaultParam);
      }
    }
  }

  return result;
}
