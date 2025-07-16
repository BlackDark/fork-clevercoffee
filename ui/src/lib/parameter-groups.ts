export const parameterGroups = [
  {
    key: "pidParameters",
    label: "PID Parameters",
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
    key: "brewControl",
    label: "Brew Control",
    parameters: ["brew.setpoint", "brew.temp_offset"],
  },
  {
    key: "steamControl",
    label: "Steam Control",
    parameters: ["steam.setpoint", "STEAM_MODE"],
  },
  {
    key: "standby",
    label: "Power Settings",
    parameters: ["standby.enabled", "standby.time"],
  },
  {
    key: "displaySettings",
    label: "Display Settings",
    parameters: [
      "display.template",
      "display.inverted",
      "display.language",
      "display.fullscreen_brew_timer",
      "display.fullscreen_manual_flush_timer",
      "display.fullscreen_hot_water_timer",
      "display.post_brew_timer_duration",
      "display.heating_logo",
      "display.pid_off_logo",
    ],
  },
  {
    key: "mqttSettings",
    label: "MQTT Settings",
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
    label: "System Settings",
    parameters: ["system.hostname", "system.ota_password", "system.log_level"],
  },
  {
    key: "systemAuth",
    label: "System Auth",
    parameters: [
      "system.auth.enabled",
      "system.auth.username",
      "system.auth.password",
    ],
  },
  {
    key: "oledDisplay",
    label: "OLED Display",
    parameters: [
      "hardware.oled.enabled",
      "hardware.oled.type",
      "hardware.oled.address",
    ],
  },
  {
    key: "relays",
    label: "Relays",
    parameters: [
      "hardware.relays.heater.trigger_type",
      "hardware.relays.valve.trigger_type",
      "hardware.relays.pump.trigger_type",
    ],
  },
  {
    key: "switchesBrew",
    label: "Brew Switches",
    parameters: [
      "hardware.switches.brew.enabled",
      "hardware.switches.brew.type",
      "hardware.switches.brew.mode",
    ],
  },
  {
    key: "switchesSteam",
    label: "Steam Switches",
    parameters: [
      "hardware.switches.steam.enabled",
      "hardware.switches.steam.type",
      "hardware.switches.steam.mode",
    ],
  },
  {
    key: "switchesPower",
    label: "Power Switches",
    parameters: [
      "hardware.switches.power.enabled",
      "hardware.switches.power.type",
      "hardware.switches.power.mode",
    ],
  },
  {
    key: "switchesHotWater",
    label: "Hot Water Switches",
    parameters: [
      "hardware.switches.hot_water.enabled",
      "hardware.switches.hot_water.type",
      "hardware.switches.hot_water.mode",
    ],
  },
  {
    key: "ledsStatus",
    label: "Status LEDs",
    parameters: [
      "hardware.leds.status.enabled",
      "hardware.leds.status.inverted",
    ],
  },
  {
    key: "ledsBrew",
    label: "Brew LEDs",
    parameters: ["hardware.leds.brew.enabled", "hardware.leds.brew.inverted"],
  },
  {
    key: "ledsSteam",
    label: "Steam LEDs",
    parameters: ["hardware.leds.steam.enabled", "hardware.leds.steam.inverted"],
  },
  {
    key: "sensorTemperature",
    label: "Temperature Sensors",
    parameters: ["hardware.sensors.temperature.type"],
  },
  {
    key: "sensorPressure",
    label: "Pressure Sensors",
    parameters: ["hardware.sensors.pressure.enabled"],
  },
  {
    key: "sensorWatertank",
    label: "Water Tank Sensors",
    parameters: [
      "hardware.sensors.watertank.enabled",
      "hardware.sensors.watertank.mode",
    ],
  },
  {
    key: "sensorScale",
    label: "Scale Sensors",
    parameters: [
      "hardware.sensors.scale.enabled",
      "hardware.sensors.scale.type",
      "hardware.sensors.scale.samples",
      "hardware.sensors.scale.calibration",
      "hardware.sensors.scale.calibration2",
      "hardware.sensors.scale.known_weight",
      "TARE_ON",
      "CALIBRATION_ON",
    ],
  },
  {
    key: "brewSection",
    label: "Brew Section",
    parameters: [
      "brew.mode",
      "brew.by_time",
      "brew.target_time",
      "brew.by_weight",
      "brew.target_weight",
      "brew.pre_infusion.enabled",
      "brew.pre_infusion.time",
      "brew.pre_infusion.pause",
    ],
  },
  {
    key: "maintenance",
    label: "Maintenance",
    parameters: [
      "backflush.cycles",
      "backflush.fill_time",
      "backflush.flush_time",
      "BACKFLUSH_ON",
    ],
  },
  {
    key: "brewPidSection",
    label: "Brew PID Section",
    parameters: [
      "pid.bd.enabled",
      "brew.pid_delay",
      "pid.bd.kp",
      "pid.bd.tn",
      "pid.bd.tv",
    ],
  },
  {
    key: "other",
    label: "Other",
    parameters: ["STEAM_MODE", "VERSION"],
  },
];
