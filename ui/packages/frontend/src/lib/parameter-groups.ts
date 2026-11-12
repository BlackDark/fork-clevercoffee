import * as m from "@/paraglide/messages.js";

export const groups2: Record<string, Array<string>> = {
  behavior: [
    "pidParameters",
    "temperatureControl",
    "brewPidSection",
    "brewControl",
    "scaleParameters",
    "displaySettings",
    "maintenance",
    "powerSettings",
  ],
  system: [
    "wifiSettings",
    "mqttSettings",
    "systemSettings",
    "systemAuth",
    "runtimeControls",
  ],
  hardware: [
    "oledDisplay",
    "relays",
    "switchesBrew",
    "switchesSteam",
    "switchesPower",
    "switchesHotWater",
    "ledsStatus",
    "ledsBrew",
    "ledsSteam",
    "sensorTemperature",
    "sensorPressure",
    "sensorWatertank",
    "sensorScale",
  ],
};

type ParameterGroup = {
  key: string;
  label: string;
  parameters: string[];
};

export const parameterGroups: ParameterGroup[] = [
  {
    key: "pidParameters",
    label: m.status_pid_parameters(),
    parameters: [
      "pid.enabled",
      "pid.use_ponm",
      "pid.ema_factor",
      "pid.regular.kp",
      "pid.regular.tn",
      "pid.regular.tv",
      "pid.regular.i_max",
      "pid.steam.kp",
    ],
  },
  {
    key: "temperatureControl",
    label: m.status_temperature_control(),
    parameters: [
      // "TEMP",
      "brew.setpoint",
      "brew.temp_offset",
      "steam.setpoint",
    ],
  },
  {
    key: "brewPidSection",
    label: m.status_brew_pid_parameters(),
    parameters: [
      "pid.bd.enabled",
      "brew.pid_delay",
      "pid.bd.kp",
      "pid.bd.tn",
      "pid.bd.tv",
    ],
  },
  {
    key: "brewControl",
    label: m.status_brew_control(),
    parameters: [
      "brew.mode",
      "brew.by_time.enabled",
      "brew.by_time.target_time",
      "brew.by_weight.enabled",
      "brew.by_weight.target_weight",
      "brew.by_weight.auto_tare",
      "brew.pre_infusion.enabled",
      "brew.pre_infusion.time",
      "brew.pre_infusion.pause",
    ],
  },
  {
    key: "scaleParameters",
    label: m.status_scale_parameters(),
    parameters: [
      // Scale parameters are listed under hardware (Scale Sensors).
    ],
  },
  {
    key: "displaySettings",
    label: m.status_display_settings(),
    parameters: [
      "display.template",
      "display.inverted",
      "display.language",
      "display.blinking.delta",
      "display.fullscreen_brew_timer",
      "display.fullscreen_manual_flush_timer",
      "display.fullscreen_hot_water_timer",
      "display.post_brew_timer_duration",
      "display.heating_logo",
      "display.pid_off_logo",
    ],
  },
  {
    key: "maintenance",
    label: m.status_maintenance(),
    parameters: [
      "backflush.cycles",
      "backflush.fill_time",
      "backflush.flush_time",
      "maintenance.backflush_reminder.enabled",
      "maintenance.backflush_reminder.threshold",
    ],
  },
  {
    key: "powerSettings",
    label: m.status_power_settings(),
    parameters: ["standby.enabled", "standby.time"],
  },
  {
    key: "wifiSettings",
    label: m.status_wifi(),
    parameters: [
      "system.wifi.ssid",
      "system.wifi.password",
      "system.offline_mode",
    ],
  },
  {
    key: "mqttSettings",
    label: m.status_mqtt_settings(),
    parameters: [
      "mqtt.enabled",
      "mqtt.broker",
      "mqtt.port",
      "mqtt.username",
      "mqtt.password",
      "mqtt.topic",
      "mqtt.hassio.enabled",
      "mqtt.hassio.prefix",
    ],
  },
  {
    key: "systemSettings",
    label: m.status_system_settings(),
    parameters: [
      "system.hostname",
      "system.ota_password",
      "system.log_level",
      "system.timing_debug.enabled",
      "system.showdisplay.enabled",
    ],
  },
  {
    key: "systemAuth",
    label: m.status_system_authentication(),
    parameters: [
      "system.auth.enabled",
      "system.auth.username",
      "system.auth.password",
    ],
  },
  {
    key: "runtimeControls",
    label: m.status_runtime_controls(),
    parameters: [
      "STEAM_MODE",
      "BACKFLUSH_ON",
      "TARE_ON",
      "CALIBRATION_ON",
      // "VERSION",
    ],
  },
  {
    key: "oledDisplay",
    label: m.status_oled_display(),
    parameters: [
      "hardware.oled.enabled",
      "hardware.oled.type",
      "hardware.oled.address",
    ],
  },
  {
    key: "relays",
    label: m.status_relays(),
    parameters: [
      "hardware.relays.heater.trigger_type",
      "hardware.relays.valve.trigger_type",
      "hardware.relays.pump.trigger_type",
    ],
  },
  {
    key: "switchesBrew",
    label: m.status_brew_switches(),
    parameters: [
      "hardware.switches.brew.enabled",
      "hardware.switches.brew.type",
      "hardware.switches.brew.mode",
    ],
  },
  {
    key: "switchesSteam",
    label: m.status_steam_switches(),
    parameters: [
      "hardware.switches.steam.enabled",
      "hardware.switches.steam.type",
      "hardware.switches.steam.mode",
    ],
  },
  {
    key: "switchesPower",
    label: m.status_power_switches(),
    parameters: [
      "hardware.switches.power.enabled",
      "hardware.switches.power.type",
      "hardware.switches.power.mode",
    ],
  },
  {
    key: "switchesHotWater",
    label: m.status_hot_water_switches(),
    parameters: [
      "hardware.switches.hot_water.enabled",
      "hardware.switches.hot_water.type",
      "hardware.switches.hot_water.mode",
    ],
  },
  {
    key: "ledsStatus",
    label: m.status_status_leds(),
    parameters: [
      "hardware.leds.status.enabled",
      "hardware.leds.status.inverted",
    ],
  },
  {
    key: "ledsBrew",
    label: m.status_brew_leds(),
    parameters: ["hardware.leds.brew.enabled", "hardware.leds.brew.inverted"],
  },
  {
    key: "ledsSteam",
    label: m.status_steam_leds(),
    parameters: ["hardware.leds.steam.enabled", "hardware.leds.steam.inverted"],
  },
  {
    key: "sensorTemperature",
    label: m.status_temperature_sensors(),
    parameters: ["hardware.sensors.temperature.type"],
  },
  {
    key: "sensorPressure",
    label: m.status_pressure_sensors(),
    parameters: ["hardware.sensors.pressure.enabled"],
  },
  {
    key: "sensorWatertank",
    label: m.status_water_tank_sensors(),
    parameters: [
      "hardware.sensors.watertank.enabled",
      "hardware.sensors.watertank.mode",
      "hardware.sensors.watertank.keep_heater_on_empty",
    ],
  },
  {
    key: "sensorScale",
    label: m.status_scale_sensors(),
    parameters: [
      "hardware.sensors.scale.enabled",
      "hardware.sensors.scale.type",
      "hardware.sensors.scale.samples",
      "hardware.sensors.scale.calibration",
      "hardware.sensors.scale.calibration2",
      "hardware.sensors.scale.known_weight",
    ],
  },
];

export const mappedParameterGroups = new Map<string, ParameterGroup>(
  parameterGroups.map((group) => [group.key, group]),
);
