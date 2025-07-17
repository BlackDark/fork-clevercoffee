// Complete parameter definitions with default values
// This ensures ALL parameters are available in the UI even if not sent by the server

import type { Parameter } from "@/types/parameters";

export const completeParameterDefinitions: Parameter[] = [
  // PID Parameters (Section 0)
  {
    type: 1, // UINT8 (boolean)
    name: "pid.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 0,
    position: 101,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "pid.use_ponm",
    value: 0,
    min: 0,
    max: 1,
    section: 0,
    position: 102,
  },
  {
    type: 2, // DOUBLE
    name: "pid.ema_factor",
    value: 0.6,
    min: 0,
    max: 1,
    section: 0,
    position: 111,
  },
  {
    type: 2, // DOUBLE
    name: "pid.regular.kp",
    value: 62,
    min: 0,
    max: 200,
    section: 0,
    position: 112,
  },
  {
    type: 2, // DOUBLE
    name: "pid.regular.tn",
    value: 52,
    min: 0,
    max: 200,
    section: 0,
    position: 113,
  },
  {
    type: 2, // DOUBLE
    name: "pid.regular.tv",
    value: 11.5,
    min: 0,
    max: 200,
    section: 0,
    position: 114,
  },
  {
    type: 2, // DOUBLE
    name: "pid.regular.i_max",
    value: 55,
    min: 0,
    max: 100,
    section: 0,
    position: 115,
  },
  {
    type: 2, // DOUBLE
    name: "pid.steam.kp",
    value: 150,
    min: 0,
    max: 500,
    section: 0,
    position: 116,
  },

  // Temperature Control (Section 1)
  {
    type: 2, // DOUBLE
    name: "brew.setpoint",
    value: 95,
    min: 20,
    max: 110,
    section: 1,
    position: 201,
  },
  {
    type: 2, // DOUBLE
    name: "brew.temp_offset",
    value: 0,
    min: 0,
    max: 20,
    section: 1,
    position: 202,
  },
  {
    type: 2, // DOUBLE
    name: "steam.setpoint",
    value: 120,
    min: 100,
    max: 140,
    section: 1,
    position: 203,
  },

  // Brew PID Parameters (Section 2)
  {
    type: 1, // UINT8 (boolean)
    name: "pid.bd.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 2,
    position: 701,
  },
  {
    type: 2, // DOUBLE
    name: "brew.pid_delay",
    value: 0,
    min: 0,
    max: 60,
    section: 2,
    position: 711,
  },
  {
    type: 2, // DOUBLE
    name: "pid.bd.kp",
    value: 0,
    min: 0,
    max: 200,
    section: 2,
    position: 712,
  },
  {
    type: 2, // DOUBLE
    name: "pid.bd.tn",
    value: 0,
    min: 0,
    max: 200,
    section: 2,
    position: 713,
  },
  {
    type: 2, // DOUBLE
    name: "pid.bd.tv",
    value: 0,
    min: 0,
    max: 200,
    section: 2,
    position: 714,
  },

  // Brew Control (Section 3) - THE MISSING PARAMETERS!
  {
    type: 5, // ENUM
    name: "brew.mode",
    value: 0,
    min: 0,
    max: 1,
    section: 3,
    position: 301,
    options: [
      { value: 0, label: "Manual" },
      { value: 1, label: "Automatic" },
    ],
  },
  {
    type: 1, // UINT8 (boolean) - HERE IS brew.by_time!
    name: "brew.by_time",
    value: 0,
    min: 0,
    max: 1,
    section: 3,
    position: 311,
  },
  {
    type: 2, // DOUBLE
    name: "brew.target_time",
    value: 25,
    min: 1,
    max: 300,
    section: 3,
    position: 312,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "brew.by_weight",
    value: 0,
    min: 0,
    max: 1,
    section: 3,
    position: 321,
  },
  {
    type: 2, // DOUBLE
    name: "brew.target_weight",
    value: 36,
    min: 1,
    max: 500,
    section: 3,
    position: 322,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "brew.pre_infusion.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 3,
    position: 331,
  },
  {
    type: 2, // DOUBLE
    name: "brew.pre_infusion.time",
    value: 2,
    min: 0,
    max: 30,
    section: 3,
    position: 332,
  },
  {
    type: 2, // DOUBLE
    name: "brew.pre_infusion.pause",
    value: 8,
    min: 0,
    max: 30,
    section: 3,
    position: 333,
  },

  // Scale Parameters (Section 4)
  {
    type: 3, // FLOAT
    name: "hardware.sensors.scale.known_weight",
    value: 267,
    min: 1,
    max: 2000,
    section: 4,
    position: 601,
  },
  {
    type: 3, // FLOAT
    name: "hardware.sensors.scale.calibration",
    value: 1,
    min: -100000,
    max: 100000,
    section: 4,
    position: 602,
  },
  {
    type: 3, // FLOAT
    name: "hardware.sensors.scale.calibration2",
    value: 1,
    min: -100000,
    max: 100000,
    section: 4,
    position: 603,
  },

  // Display Settings (Section 5)
  {
    type: 5, // ENUM
    name: "display.template",
    value: 0,
    min: 0,
    max: 4,
    section: 5,
    position: 901,
    options: [
      { value: 0, label: "Standard" },
      { value: 1, label: "Minimal" },
      { value: 2, label: "Temp only" },
      { value: 3, label: "Scale" },
      { value: 4, label: "Upright" },
    ],
  },
  {
    type: 1, // UINT8 (boolean)
    name: "display.inverted",
    value: 0,
    min: 0,
    max: 1,
    section: 5,
    position: 902,
  },
  {
    type: 5, // ENUM
    name: "display.language",
    value: 0,
    min: 0,
    max: 2,
    section: 5,
    position: 903,
    options: [
      { value: 0, label: "Deutsch" },
      { value: 1, label: "English" },
      { value: 2, label: "Español" },
    ],
  },
  {
    type: 1, // UINT8 (boolean)
    name: "display.fullscreen_brew_timer",
    value: 0,
    min: 0,
    max: 1,
    section: 5,
    position: 904,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "display.fullscreen_manual_flush_timer",
    value: 0,
    min: 0,
    max: 1,
    section: 5,
    position: 905,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "display.fullscreen_hot_water_timer",
    value: 0,
    min: 0,
    max: 1,
    section: 5,
    position: 906,
  },
  {
    type: 2, // DOUBLE
    name: "display.post_brew_timer_duration",
    value: 3,
    min: 0,
    max: 60,
    section: 5,
    position: 907,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "display.heating_logo",
    value: 1,
    min: 0,
    max: 1,
    section: 5,
    position: 908,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "display.pid_off_logo",
    value: 1,
    min: 0,
    max: 1,
    section: 5,
    position: 909,
  },

  // Maintenance (Section 6)
  {
    type: 0, // INTEGER
    name: "backflush.cycles",
    value: 5,
    min: 1,
    max: 20,
    section: 6,
    position: 401,
  },
  {
    type: 2, // DOUBLE
    name: "backflush.fill_time",
    value: 5,
    min: 1,
    max: 30,
    section: 6,
    position: 402,
  },
  {
    type: 2, // DOUBLE
    name: "backflush.flush_time",
    value: 10,
    min: 1,
    max: 30,
    section: 6,
    position: 403,
  },

  // Power Settings (Section 7)
  {
    type: 1, // UINT8 (boolean)
    name: "standby.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 7,
    position: 801,
  },
  {
    type: 2, // DOUBLE
    name: "standby.time",
    value: 35,
    min: 1,
    max: 120,
    section: 7,
    position: 802,
  },

  // MQTT Settings (Section 8)
  {
    type: 1, // UINT8 (boolean)
    name: "mqtt.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 8,
    position: 1001,
  },
  {
    type: 4, // STRING
    name: "mqtt.broker",
    value: "",
    min: 0,
    max: 64,
    section: 8,
    position: 1011,
  },
  {
    type: 0, // INTEGER
    name: "mqtt.port",
    value: 1883,
    min: 1,
    max: 65535,
    section: 8,
    position: 1012,
  },
  {
    type: 4, // STRING
    name: "mqtt.username",
    value: "rancilio",
    min: 0,
    max: 32,
    section: 8,
    position: 1013,
  },
  {
    type: 4, // STRING
    name: "mqtt.password",
    value: "silvia",
    min: 0,
    max: 64,
    section: 8,
    position: 1014,
  },
  {
    type: 4, // STRING
    name: "mqtt.topic",
    value: "custom/kitchen/",
    min: 0,
    max: 48,
    section: 8,
    position: 1015,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "mqtt.hassio.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 8,
    position: 1021,
  },
  {
    type: 4, // STRING
    name: "mqtt.hassio.prefix",
    value: "homeassistant",
    min: 0,
    max: 24,
    section: 8,
    position: 1022,
  },

  // System Settings (Section 9)
  {
    type: 4, // STRING
    name: "system.hostname",
    value: "silvia",
    min: 0,
    max: 64,
    section: 9,
    position: 1101,
  },
  {
    type: 4, // STRING
    name: "system.ota_password",
    value: "otapass",
    min: 0,
    max: 64,
    section: 9,
    position: 1102,
  },
  {
    type: 5, // ENUM
    name: "system.log_level",
    value: 2,
    min: 0,
    max: 6,
    section: 9,
    position: 1103,
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
    type: 1, // UINT8 (boolean)
    name: "system.auth.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 9,
    position: 1201,
  },
  {
    type: 4, // STRING
    name: "system.auth.username",
    value: "admin",
    min: 0,
    max: 32,
    section: 9,
    position: 1202,
  },
  {
    type: 4, // STRING
    name: "system.auth.password",
    value: "admin",
    min: 0,
    max: 64,
    section: 9,
    position: 1203,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "system.timing_debug.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 9,
    position: 1301,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "system.showdisplay.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 9,
    position: 1303,
  },

  // Runtime Controls (Section 10)
  {
    type: 1, // UINT8 (boolean)
    name: "STEAM_MODE",
    value: 0,
    min: 0,
    max: 1,
    section: 10,
    position: 503,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "BACKFLUSH_ON",
    value: 0,
    min: 0,
    max: 1,
    section: 10,
    position: 504,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "TARE_ON",
    value: 0,
    min: 0,
    max: 1,
    section: 10,
    position: 501,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "CALIBRATION_ON",
    value: 0,
    min: 0,
    max: 1,
    section: 10,
    position: 502,
  },

  // Hardware - OLED Display (Section 11)
  {
    type: 1, // UINT8 (boolean)
    name: "hardware.oled.enabled",
    value: 1,
    min: 0,
    max: 1,
    section: 11,
    position: 2001,
  },
  {
    type: 5, // ENUM
    name: "hardware.oled.type",
    value: 0,
    min: 0,
    max: 1,
    section: 11,
    position: 2002,
    options: [
      { value: 0, label: "SH1106" },
      { value: 1, label: "SSD1306" },
    ],
  },
  {
    type: 5, // ENUM
    name: "hardware.oled.address",
    value: 0,
    min: 0,
    max: 1,
    section: 11,
    position: 2003,
    options: [
      { value: 0, label: "0x3C" },
      { value: 1, label: "0x3D" },
    ],
  },

  // Hardware - Relays (Section 12)
  {
    type: 5, // ENUM
    name: "hardware.relays.heater.trigger_type",
    value: 1,
    min: 0,
    max: 1,
    section: 12,
    position: 2101,
    options: [
      { value: 0, label: "Low Trigger" },
      { value: 1, label: "High Trigger" },
    ],
  },
  {
    type: 5, // ENUM
    name: "hardware.relays.valve.trigger_type",
    value: 1,
    min: 0,
    max: 1,
    section: 12,
    position: 2102,
    options: [
      { value: 0, label: "Low Trigger" },
      { value: 1, label: "High Trigger" },
    ],
  },
  {
    type: 5, // ENUM
    name: "hardware.relays.pump.trigger_type",
    value: 1,
    min: 0,
    max: 1,
    section: 12,
    position: 2103,
    options: [
      { value: 0, label: "Low Trigger" },
      { value: 1, label: "High Trigger" },
    ],
  },

  // Hardware - Switches (Section 13)
  {
    type: 1, // UINT8 (boolean)
    name: "hardware.switches.brew.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 13,
    position: 2201,
  },
  {
    type: 5, // ENUM
    name: "hardware.switches.brew.type",
    value: 1,
    min: 0,
    max: 1,
    section: 13,
    position: 2202,
    options: [
      { value: 0, label: "Momentary" },
      { value: 1, label: "Toggle" },
    ],
  },
  {
    type: 5, // ENUM
    name: "hardware.switches.brew.mode",
    value: 0,
    min: 0,
    max: 1,
    section: 13,
    position: 2203,
    options: [
      { value: 0, label: "Normally Open" },
      { value: 1, label: "Normally Closed" },
    ],
  },
  {
    type: 1, // UINT8 (boolean)
    name: "hardware.switches.steam.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 13,
    position: 2211,
  },
  {
    type: 5, // ENUM
    name: "hardware.switches.steam.type",
    value: 1,
    min: 0,
    max: 1,
    section: 13,
    position: 2212,
    options: [
      { value: 0, label: "Momentary" },
      { value: 1, label: "Toggle" },
    ],
  },
  {
    type: 5, // ENUM
    name: "hardware.switches.steam.mode",
    value: 0,
    min: 0,
    max: 1,
    section: 13,
    position: 2213,
    options: [
      { value: 0, label: "Normally Open" },
      { value: 1, label: "Normally Closed" },
    ],
  },
  {
    type: 1, // UINT8 (boolean)
    name: "hardware.switches.power.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 13,
    position: 2221,
  },
  {
    type: 5, // ENUM
    name: "hardware.switches.power.type",
    value: 1,
    min: 0,
    max: 1,
    section: 13,
    position: 2222,
    options: [
      { value: 0, label: "Momentary" },
      { value: 1, label: "Toggle" },
    ],
  },
  {
    type: 5, // ENUM
    name: "hardware.switches.power.mode",
    value: 0,
    min: 0,
    max: 1,
    section: 13,
    position: 2223,
    options: [
      { value: 0, label: "Normally Open" },
      { value: 1, label: "Normally Closed" },
    ],
  },
  {
    type: 1, // UINT8 (boolean)
    name: "hardware.switches.hot_water.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 13,
    position: 2231,
  },
  {
    type: 5, // ENUM
    name: "hardware.switches.hot_water.type",
    value: 1,
    min: 0,
    max: 1,
    section: 13,
    position: 2232,
    options: [
      { value: 0, label: "Momentary" },
      { value: 1, label: "Toggle" },
    ],
  },
  {
    type: 5, // ENUM
    name: "hardware.switches.hot_water.mode",
    value: 0,
    min: 0,
    max: 1,
    section: 13,
    position: 2233,
    options: [
      { value: 0, label: "Normally Open" },
      { value: 1, label: "Normally Closed" },
    ],
  },

  // Hardware - LEDs (Section 14)
  {
    type: 1, // UINT8 (boolean)
    name: "hardware.leds.status.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 14,
    position: 2301,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "hardware.leds.status.inverted",
    value: 0,
    min: 0,
    max: 1,
    section: 14,
    position: 2302,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "hardware.leds.brew.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 14,
    position: 2311,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "hardware.leds.brew.inverted",
    value: 0,
    min: 0,
    max: 1,
    section: 14,
    position: 2312,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "hardware.leds.steam.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 14,
    position: 2321,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "hardware.leds.steam.inverted",
    value: 0,
    min: 0,
    max: 1,
    section: 14,
    position: 2322,
  },

  // Hardware - Sensors (Section 15)
  {
    type: 5, // ENUM
    name: "hardware.sensors.temperature.type",
    value: 0,
    min: 0,
    max: 1,
    section: 15,
    position: 2401,
    options: [
      { value: 0, label: "TSIC306" },
      { value: 1, label: "Dallas DS18B20" },
    ],
  },
  {
    type: 1, // UINT8 (boolean)
    name: "hardware.sensors.pressure.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 15,
    position: 2411,
  },
  {
    type: 1, // UINT8 (boolean)
    name: "hardware.sensors.watertank.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 15,
    position: 2421,
  },
  {
    type: 5, // ENUM
    name: "hardware.sensors.watertank.mode",
    value: 1,
    min: 0,
    max: 1,
    section: 15,
    position: 2422,
    options: [
      { value: 0, label: "Normally Open" },
      { value: 1, label: "Normally Closed" },
    ],
  },
  {
    type: 1, // UINT8 (boolean)
    name: "hardware.sensors.scale.enabled",
    value: 0,
    min: 0,
    max: 1,
    section: 15,
    position: 2431,
  },
  {
    type: 5, // ENUM
    name: "hardware.sensors.scale.type",
    value: 0,
    min: 0,
    max: 1,
    section: 15,
    position: 2432,
    options: [
      { value: 0, label: "2 load cells" },
      { value: 1, label: "1 load cell" },
    ],
  },
  {
    type: 0, // INTEGER
    name: "hardware.sensors.scale.samples",
    value: 2,
    min: 1,
    max: 20,
    section: 15,
    position: 2433,
  },
  {
    type: 2, // DOUBLE
    name: "hardware.sensors.scale.calibration",
    value: 1,
    min: -10000,
    max: 10000,
    section: 15,
    position: 2434,
  },
  {
    type: 2, // DOUBLE
    name: "hardware.sensors.scale.calibration2",
    value: 1,
    min: -10000,
    max: 10000,
    section: 15,
    position: 2435,
  },
  {
    type: 2, // DOUBLE
    name: "hardware.sensors.scale.known_weight",
    value: 267,
    min: 1,
    max: 5000,
    section: 15,
    position: 2436,
  },
];

/**
 * Merges server parameters with complete parameter definitions
 * This ensures ALL parameters are available with default values
 */
export function mergeParametersWithDefaults(
  serverParameters: Parameter[]
): Parameter[] {
  const serverParamMap = new Map(serverParameters.map((p) => [p.name, p]));

  return completeParameterDefinitions.map((defaultParam) => {
    const serverParam = serverParamMap.get(defaultParam.name);
    if (serverParam) {
      // Use server values but keep our complete definition structure
      return {
        ...defaultParam,
        value: serverParam.value,
        // Keep other server properties if they exist
        ...serverParam,
      };
    }
    // Return default parameter if not found on server
    return defaultParam;
  });
}

/**
 * Evaluates parameter visibility conditions
 */
export function shouldShowParameter(
  param: Parameter,
  allParams: Parameter[]
): boolean {
  const paramMap = new Map(allParams.map((p) => [p.name, p.value]));

  // Get dependency values
  const mqttEnabled = paramMap.get("mqtt.enabled") === 1;
  const brewSwitchEnabled =
    paramMap.get("hardware.switches.brew.enabled") === 1;
  const scaleEnabled = paramMap.get("hardware.sensors.scale.enabled") === 1;
  const authEnabled = paramMap.get("system.auth.enabled") === 1;
  const brewModeAutomatic = paramMap.get("brew.mode") === 1;
  const oledEnabled = paramMap.get("hardware.oled.enabled") === 1;
  const statusLedEnabled = paramMap.get("hardware.leds.status.enabled") === 1;
  const brewLedEnabled = paramMap.get("hardware.leds.brew.enabled") === 1;
  const steamLedEnabled = paramMap.get("hardware.leds.steam.enabled") === 1;
  const steamSwitchEnabled =
    paramMap.get("hardware.switches.steam.enabled") === 1;
  const powerSwitchEnabled =
    paramMap.get("hardware.switches.power.enabled") === 1;
  const hotWaterSwitchEnabled =
    paramMap.get("hardware.switches.hot_water.enabled") === 1;
  const watertankEnabled =
    paramMap.get("hardware.sensors.watertank.enabled") === 1;
  const scaleType = paramMap.get("hardware.sensors.scale.type");
  const debugLogLevel = paramMap.get("system.log_level") === 1;
  const hassioEnabled = paramMap.get("mqtt.hassio.enabled") === 1;

  // Apply conditional logic
  // MQTT sub-parameters
  if (
    param.name.startsWith("mqtt.") &&
    param.name !== "mqtt.enabled" &&
    !mqttEnabled
  )
    return false;
  if (param.name === "mqtt.hassio.prefix" && (!mqttEnabled || !hassioEnabled))
    return false;

  // Brew-related parameters
  if (
    param.name.startsWith("brew.") &&
    !["brew.setpoint", "brew.temp_offset"].includes(param.name) &&
    !brewSwitchEnabled
  )
    return false;
  if (
    [
      "brew.by_time",
      "brew.target_time",
      "brew.by_weight",
      "brew.target_weight",
    ].includes(param.name) &&
    (!brewSwitchEnabled || !brewModeAutomatic)
  )
    return false;
  if (
    ["brew.by_weight", "brew.target_weight"].includes(param.name) &&
    !scaleEnabled
  )
    return false;

  // PID brew detection parameters
  if (param.name.startsWith("pid.bd.") && !brewSwitchEnabled) return false;

  // Backflush parameters
  if (param.name.startsWith("backflush.") && !brewSwitchEnabled) return false;
  if (param.name === "BACKFLUSH_ON" && !brewSwitchEnabled) return false;

  // Scale parameters
  if (
    param.name.startsWith("hardware.sensors.scale.") &&
    param.name !== "hardware.sensors.scale.enabled" &&
    !scaleEnabled
  )
    return false;
  if (["TARE_ON", "CALIBRATION_ON"].includes(param.name) && !scaleEnabled)
    return false;
  if (
    param.name === "hardware.sensors.scale.calibration2" &&
    (!scaleEnabled || scaleType !== 0)
  )
    return false;

  // Auth parameters
  if (
    ["system.auth.username", "system.auth.password"].includes(param.name) &&
    !authEnabled
  )
    return false;

  // OLED sub-parameters
  if (
    ["hardware.oled.type", "hardware.oled.address"].includes(param.name) &&
    !oledEnabled
  )
    return false;

  // LED sub-parameters
  if (param.name === "hardware.leds.status.inverted" && !statusLedEnabled)
    return false;
  if (param.name === "hardware.leds.brew.inverted" && !brewLedEnabled)
    return false;
  if (param.name === "hardware.leds.steam.inverted" && !steamLedEnabled)
    return false;

  // Switch sub-parameters
  if (
    ["hardware.switches.brew.type", "hardware.switches.brew.mode"].includes(
      param.name
    ) &&
    !brewSwitchEnabled
  )
    return false;
  if (
    ["hardware.switches.steam.type", "hardware.switches.steam.mode"].includes(
      param.name
    ) &&
    !steamSwitchEnabled
  )
    return false;
  if (
    ["hardware.switches.power.type", "hardware.switches.power.mode"].includes(
      param.name
    ) &&
    !powerSwitchEnabled
  )
    return false;
  if (
    [
      "hardware.switches.hot_water.type",
      "hardware.switches.hot_water.mode",
    ].includes(param.name) &&
    !hotWaterSwitchEnabled
  )
    return false;

  // Water tank sensor mode
  if (param.name === "hardware.sensors.watertank.mode" && !watertankEnabled)
    return false;

  // Debug parameters
  if (
    ["system.timing_debug.enabled", "system.showdisplay.enabled"].includes(
      param.name
    ) &&
    !debugLogLevel
  )
    return false;

  return true;
}
