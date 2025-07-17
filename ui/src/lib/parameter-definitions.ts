// Unified parameter definitions with default values and conditions
// This file consolidates all parameter definitions and replaces both all-parameters.ts and complete-parameters.ts

import type { Parameter } from "./parameter-types";
import { ParameterTypes } from "./parameter-types";

export const parameterDefinitions: Parameter[] = [
  // PID Parameters (Section 0)
  {
    type: ParameterTypes.UINT8,
    name: "pid.enabled",
    displayName: "Enable PID Controller",
    section: 0,
    position: 101,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 1,
  },
  {
    type: ParameterTypes.UINT8,
    name: "pid.use_ponm",
    displayName: "Enable PonM",
    section: 0,
    position: 102,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 1,
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "pid.ema_factor",
    displayName: "PID EMA Factor",
    section: 0,
    position: 111,
    hasHelpText: true,
    show: true,
    value: 0.6,
    min: 0,
    max: 1,
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "pid.regular.kp",
    displayName: "PID Kp",
    section: 0,
    position: 112,
    hasHelpText: true,
    show: true,
    value: 62,
    min: 0,
    max: 200,
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "pid.regular.tn",
    displayName: "PID Tn (=Kp/Ki)",
    section: 0,
    position: 113,
    hasHelpText: true,
    show: true,
    value: 52,
    min: 0,
    max: 200,
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "pid.regular.tv",
    displayName: "PID Tv (=Kd/Kp)",
    section: 0,
    position: 114,
    hasHelpText: true,
    show: true,
    value: 11.5,
    min: 0,
    max: 200,
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "pid.regular.i_max",
    displayName: "PID Integrator Max",
    section: 0,
    position: 115,
    hasHelpText: true,
    show: true,
    value: 55,
    min: 0,
    max: 100,
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "pid.steam.kp",
    displayName: "Steam Kp",
    section: 0,
    position: 116,
    hasHelpText: true,
    show: true,
    value: 150,
    min: 0,
    max: 500,
  },

  // Temperature Control (Section 1)
  {
    type: ParameterTypes.DOUBLE,
    name: "TEMP",
    displayName: "Temperature",
    section: 1,
    position: 200,
    hasHelpText: false,
    show: true,
    value: 95,
    min: 0,
    max: 200,
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "brew.setpoint",
    displayName: "Setpoint (°C)",
    section: 1,
    position: 201,
    hasHelpText: true,
    show: true,
    value: 95,
    min: 20,
    max: 110,
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "brew.temp_offset",
    displayName: "Offset (°C)",
    section: 1,
    position: 202,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 20,
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "steam.setpoint",
    displayName: "Steam Setpoint (°C)",
    section: 1,
    position: 203,
    hasHelpText: true,
    show: true,
    value: 120,
    min: 100,
    max: 140,
  },

  // Brew PID Parameters (Section 2)
  {
    type: ParameterTypes.UINT8,
    name: "pid.bd.enabled",
    displayName: "Enable Brew PID",
    section: 2,
    position: 701,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 1,
    conditions: {
      showWhen: {
        "hardware.switches.brew.enabled": 1,
      },
    },
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "brew.pid_delay",
    displayName: "Brew PID Delay (s)",
    section: 2,
    position: 711,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 60,
    conditions: {
      showWhen: {
        "hardware.switches.brew.enabled": 1,
      },
    },
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "pid.bd.kp",
    displayName: "BD Kp",
    section: 2,
    position: 712,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 200,
    conditions: {
      showWhen: {
        "hardware.switches.brew.enabled": 1,
      },
    },
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "pid.bd.tn",
    displayName: "BD Tn (=Kp/Ki)",
    section: 2,
    position: 713,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 200,
    conditions: {
      showWhen: {
        "hardware.switches.brew.enabled": 1,
      },
    },
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "pid.bd.tv",
    displayName: "BD Tv (=Kd/Kp)",
    section: 2,
    position: 714,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 200,
    conditions: {
      showWhen: {
        "hardware.switches.brew.enabled": 1,
      },
    },
  },

  // Brew Control (Section 3)
  {
    type: ParameterTypes.ENUM,
    name: "brew.mode",
    displayName: "Brew Mode",
    section: 3,
    position: 301,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 1,
    options: [
      { value: 0, label: "Manual" },
      { value: 1, label: "Automatic" },
    ],
  },
  {
    type: ParameterTypes.UINT8,
    name: "brew.by_time",
    displayName: "Brew by Time",
    section: 3,
    position: 311,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 1,
    conditions: {
      showWhen: {
        "brew.mode": 1,
      },
    },
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "brew.target_time",
    displayName: "Target Brew Time (s)",
    section: 3,
    position: 312,
    hasHelpText: true,
    show: true,
    value: 25,
    min: 1,
    max: 300,
    conditions: {
      showWhen: {
        "brew.by_time": 1,
      },
    },
  },
  {
    type: ParameterTypes.UINT8,
    name: "brew.by_weight",
    displayName: "Brew by Weight",
    section: 3,
    position: 321,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 1,
    conditions: {
      showWhen: {
        "brew.mode": 1,
      },
    },
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "brew.target_weight",
    displayName: "Target Brew Weight (g)",
    section: 3,
    position: 322,
    hasHelpText: true,
    show: true,
    value: 36,
    min: 1,
    max: 500,
    conditions: {
      showWhen: {
        "brew.by_weight": 1,
      },
    },
  },
  {
    type: ParameterTypes.UINT8,
    name: "brew.pre_infusion.enabled",
    displayName: "Pre-Infusion",
    section: 3,
    position: 331,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 1,
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "brew.pre_infusion.time",
    displayName: "Pre-infusion Time (s)",
    section: 3,
    position: 332,
    hasHelpText: true,
    show: true,
    value: 2,
    min: 0,
    max: 30,
    conditions: {
      showWhen: {
        "brew.pre_infusion.enabled": 1,
      },
    },
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "brew.pre_infusion.pause",
    displayName: "Pre-infusion Pause Time (s)",
    section: 3,
    position: 333,
    hasHelpText: true,
    show: true,
    value: 8,
    min: 0,
    max: 30,
    conditions: {
      showWhen: {
        "brew.pre_infusion.enabled": 1,
      },
    },
  },

  // Scale Parameters (Section 4)
  {
    type: ParameterTypes.FLOAT,
    name: "hardware.sensors.scale.known_weight",
    displayName: "Known Calibration Weight",
    section: 4,
    position: 601,
    hasHelpText: true,
    show: true,
    value: 267,
    min: 1,
    max: 5000,
    conditions: {
      showWhen: {
        "hardware.sensors.scale.enabled": 1,
      },
    },
  },
  {
    type: ParameterTypes.FLOAT,
    name: "hardware.sensors.scale.calibration",
    displayName: "Scale Calibration Factor",
    section: 4,
    position: 602,
    hasHelpText: true,
    show: true,
    value: 1,
    min: -100000,
    max: 100000,
    conditions: {
      showWhen: {
        "hardware.sensors.scale.enabled": 1,
      },
    },
  },
  {
    type: ParameterTypes.FLOAT,
    name: "hardware.sensors.scale.calibration2",
    displayName: "Scale Calibration Factor 2",
    section: 4,
    position: 603,
    hasHelpText: true,
    show: true,
    value: 1,
    min: -10000,
    max: 10000,
    conditions: {
      showWhen: {
        "hardware.sensors.scale.enabled": 1,
        "hardware.sensors.scale.type": 0,
      },
    },
  },

  // Display Settings (Section 5)
  {
    type: ParameterTypes.ENUM,
    name: "display.template",
    displayName: "Display Template",
    section: 5,
    position: 901,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 4,
    options: [
      { value: 0, label: "Standard" },
      { value: 1, label: "Minimal" },
      { value: 2, label: "Temp only" },
      { value: 3, label: "Scale" },
      { value: 4, label: "Upright" },
    ],
  },
  {
    type: ParameterTypes.UINT8,
    name: "display.inverted",
    displayName: "Invert Display",
    section: 5,
    position: 902,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 1,
  },
  {
    type: ParameterTypes.ENUM,
    name: "display.language",
    displayName: "Display Language",
    section: 5,
    position: 903,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 2,
    options: [
      { value: 0, label: "Deutsch" },
      { value: 1, label: "English" },
      { value: 2, label: "Español" },
    ],
  },
  {
    type: ParameterTypes.UINT8,
    name: "display.fullscreen_brew_timer",
    displayName: "Enable Fullscreen Brew Timer",
    section: 5,
    position: 904,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 1,
  },
  {
    type: ParameterTypes.UINT8,
    name: "display.fullscreen_manual_flush_timer",
    displayName: "Enable Fullscreen Manual Flush Timer",
    section: 5,
    position: 905,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 1,
  },
  {
    type: ParameterTypes.UINT8,
    name: "display.fullscreen_hot_water_timer",
    displayName: "Enable Fullscreen Hot Water Timer",
    section: 5,
    position: 906,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 1,
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "display.post_brew_timer_duration",
    displayName: "Post Brew Timer Duration (s)",
    section: 5,
    position: 907,
    hasHelpText: true,
    show: true,
    value: 3,
    min: 0,
    max: 60,
  },

  // Maintenance (Section 6)
  {
    type: ParameterTypes.INTEGER,
    name: "backflush.cycles",
    displayName: "Backflush Cycles",
    section: 6,
    position: 401,
    hasHelpText: true,
    show: true,
    value: 5,
    min: 1,
    max: 20,
    conditions: {
      showWhen: {
        "hardware.switches.brew.enabled": 1,
      },
    },
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "backflush.fill_time",
    displayName: "Backflush Fill Time (s)",
    section: 6,
    position: 402,
    hasHelpText: true,
    show: true,
    value: 5,
    min: 1,
    max: 30,
    conditions: {
      showWhen: {
        "hardware.switches.brew.enabled": 1,
      },
    },
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "backflush.flush_time",
    displayName: "Backflush Flush Time (s)",
    section: 6,
    position: 403,
    hasHelpText: true,
    show: true,
    value: 10,
    min: 1,
    max: 30,
    conditions: {
      showWhen: {
        "hardware.switches.brew.enabled": 1,
      },
    },
  },

  // Power Settings (Section 7)
  {
    type: ParameterTypes.UINT8,
    name: "standby.enabled",
    displayName: "Enable Standby Mode",
    section: 7,
    position: 801,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 1,
  },
  {
    type: ParameterTypes.DOUBLE,
    name: "standby.time",
    displayName: "Standby Time (min)",
    section: 7,
    position: 802,
    hasHelpText: true,
    show: true,
    value: 35,
    min: 1,
    max: 120,
    conditions: {
      showWhen: {
        "standby.enabled": 1,
      },
    },
  },

  // MQTT Settings (Section 8)
  {
    type: ParameterTypes.UINT8,
    name: "mqtt.enabled",
    displayName: "Enable MQTT",
    section: 8,
    position: 901,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 1,
  },
  {
    type: ParameterTypes.STRING,
    name: "mqtt.broker",
    displayName: "MQTT Broker",
    section: 8,
    position: 902,
    hasHelpText: true,
    show: true,
    value: "",
    min: 0,
    max: 64,
    conditions: {
      showWhen: {
        "mqtt.enabled": 1,
      },
    },
  },

  // Runtime Controls (Section 10)
  {
    type: ParameterTypes.UINT8,
    name: "STEAM_MODE",
    displayName: "Steam Mode",
    section: 10,
    position: 503,
    hasHelpText: false,
    show: true,
    value: 0,
    min: 0,
    max: 1,
  },
  {
    type: ParameterTypes.UINT8,
    name: "BACKFLUSH_ON",
    displayName: "Backflush",
    section: 10,
    position: 504,
    hasHelpText: false,
    show: true,
    value: 0,
    min: 0,
    max: 1,
    conditions: {
      showWhen: {
        "hardware.switches.brew.enabled": 1,
      },
    },
  },
  {
    type: ParameterTypes.UINT8,
    name: "TARE_ON",
    displayName: "Tare",
    section: 10,
    position: 501,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 1,
    conditions: {
      showWhen: {
        "hardware.sensors.scale.enabled": 1,
      },
    },
  },
  {
    type: ParameterTypes.UINT8,
    name: "CALIBRATION_ON",
    displayName: "Calibration",
    section: 10,
    position: 502,
    hasHelpText: true,
    show: true,
    value: 0,
    min: 0,
    max: 1,
    conditions: {
      showWhen: {
        "hardware.sensors.scale.enabled": 1,
      },
    },
  },
];

/**
 * Merges server parameters with complete parameter definitions
 * This ensures ALL parameters are available with default values
 */
export function mergeParametersWithDefaults(
  serverParameters: Parameter[]
): Parameter[] {
  const serverParamMap = new Map(
    serverParameters.map((param) => [param.name, param])
  );

  return parameterDefinitions.map((defaultParam) => {
    const serverParam = serverParamMap.get(defaultParam.name);
    if (serverParam) {
      // Merge server data with defaults, keeping server values
      return {
        ...defaultParam,
        ...serverParam,
        displayName: defaultParam.displayName || serverParam.name,
        hasHelpText: defaultParam.hasHelpText,
        conditions: defaultParam.conditions,
      };
    }
    return defaultParam;
  });
}

/**
 * Get parameter by name
 */
export function getParameterByName(name: string): Parameter | undefined {
  return parameterDefinitions.find((param) => param.name === name);
}
