// Parameter label translations - efficient Map-based structure
// This provides localized display names for parameters

// English labels map for fast lookups
export const parameterLabelsEN = new Map<string, string>([
  // PID Parameters
  ["pid.enabled", "Enable PID Controller"],
  ["pid.use_ponm", "Enable PonM"],
  ["pid.ema_factor", "PID EMA Factor"],
  ["pid.regular.kp", "PID Kp"],
  ["pid.regular.tn", "PID Tn (=Kp/Ki)"],
  ["pid.regular.tv", "PID Tv (=Kd/Kp)"],
  ["pid.regular.i_max", "PID Integrator Max"],
  ["pid.steam.kp", "Steam Kp"],

  // Temperature Control
  ["TEMP", "Temperature"],
  ["brew.setpoint", "Setpoint (°C)"],
  ["brew.temp_offset", "Offset (°C)"],
  ["steam.setpoint", "Steam Setpoint (°C)"],

  // Brew PID Parameters
  ["pid.bd.enabled", "Enable Brew PID"],
  ["brew.pid_delay", "Brew PID Delay (s)"],
  ["pid.bd.kp", "BD Kp"],
  ["pid.bd.tn", "BD Tn (=Kp/Ki)"],
  ["pid.bd.tv", "BD Tv (=Kd/Kp)"],

  // Brew Control
  ["brew.mode", "Brew Mode"],
  ["brew.by_time.enabled", "Brew by Time"],
  ["brew.by_time.target_time", "Target Brew Time (s)"],
  ["brew.by_weight.enabled", "Brew by Weight"],
  ["brew.by_weight.target_weight", "Target Brew Weight (g)"],
  ["brew.by_weight.auto_tare", "Auto-tare"],
  ["brew.pre_infusion.enabled", "Pre-Infusion"],
  ["brew.pre_infusion.time", "Pre-infusion Time (s)"],
  ["brew.pre_infusion.pause", "Pre-infusion Pause Time (s)"],

  // Scale Parameters
  ["hardware.sensors.scale.known_weight", "Known Calibration Weight"],
  ["hardware.sensors.scale.calibration", "Scale Calibration Factor"],
  ["hardware.sensors.scale.calibration2", "Scale Calibration Factor 2"],

  // Display Settings
  ["display.template", "Display Template"],
  ["display.inverted", "Invert Display"],
  ["display.language", "Display Language"],
  ["display.blinking.delta", "Status LED Delta"],
  ["display.fullscreen_brew_timer", "Enable Fullscreen Brew Timer"],
  [
    "display.fullscreen_manual_flush_timer",
    "Enable Fullscreen Manual Flush Timer",
  ],
  ["display.fullscreen_hot_water_timer", "Enable Fullscreen Hot Water Timer"],
  ["display.post_brew_timer_duration", "Post Brew Timer Duration (s)"],
  ["display.heating_logo", "Enable Heating Logo"],
  ["display.pid_off_logo", "Enable 'PID Disabled' Logo"],

  // Maintenance
  ["backflush.cycles", "Backflush Cycles"],
  ["backflush.fill_time", "Backflush Fill Time (s)"],
  ["backflush.flush_time", "Backflush Flush Time (s)"],
  ["maintenance.backflush_reminder.enabled", "Backflush Reminder"],
  ["maintenance.backflush_reminder.threshold", "Backflush Reminder Threshold"],

  // Power Settings
  ["standby.enabled", "Enable Standby Timer"],
  ["standby.time", "Standby Time"],

  // MQTT Settings
  ["mqtt.enabled", "MQTT enabled"],
  ["mqtt.broker", "Hostname"],
  ["mqtt.port", "Port"],
  ["mqtt.username", "Username"],
  ["mqtt.password", "Password"],
  ["mqtt.topic", "Topic Prefix"],
  ["mqtt.hassio.enabled", "Hass.io enabled"],
  ["mqtt.hassio.prefix", "Hass.io Prefix"],

  // System Settings
  ["system.hostname", "Hostname"],
  ["system.ota_password", "OTA Password"],
  ["system.log_level", "Log Level"],
  ["system.timing_debug.enabled", "Loop timing in console"],
  ["system.showdisplay.enabled", "Activate display recording in debug logs"],

  // System Authentication
  ["system.auth.enabled", "Enable Website Authentication"],
  ["system.auth.username", "Website Username"],
  ["system.auth.password", "Website Password"],

  // Runtime Controls
  ["STEAM_MODE", "Steam Mode"],
  ["BACKFLUSH_ON", "Backflush"],
  ["TARE_ON", "Tare"],
  ["CALIBRATION_ON", "Calibration"],
  ["VERSION", "Version"],

  // Hardware - OLED Display
  ["hardware.oled.enabled", "Enable OLED Display"],
  ["hardware.oled.type", "OLED Type"],
  ["hardware.oled.address", "I2C Address"],

  // Hardware - Relays
  ["hardware.relays.heater.trigger_type", "Heater Relay Trigger Type"],
  ["hardware.relays.valve.trigger_type", "Valve Relay Trigger Type"],
  ["hardware.relays.pump.trigger_type", "Pump Relay Trigger Type"],

  // Hardware - Switches
  ["hardware.switches.brew.enabled", "Enable Brew Switch"],
  ["hardware.switches.brew.type", "Brew Switch Type"],
  ["hardware.switches.brew.mode", "Brew Switch Mode"],
  ["hardware.switches.steam.enabled", "Enable Steam Switch"],
  ["hardware.switches.steam.type", "Steam Switch Type"],
  ["hardware.switches.steam.mode", "Steam Switch Mode"],
  ["hardware.switches.power.enabled", "Enable Power Switch"],
  ["hardware.switches.power.type", "Power Switch Type"],
  ["hardware.switches.power.mode", "Power Switch Mode"],
  ["hardware.switches.hot_water.enabled", "Enable Water Switch"],
  ["hardware.switches.hot_water.type", "Water Switch Type"],
  ["hardware.switches.hot_water.mode", "Water Switch Mode"],

  // Hardware - LEDs
  ["hardware.leds.status.enabled", "Enable Status LED"],
  ["hardware.leds.status.inverted", "Invert Status LED"],
  ["hardware.leds.brew.enabled", "Enable Brew LED"],
  ["hardware.leds.brew.inverted", "Invert Brew LED"],
  ["hardware.leds.steam.enabled", "Enable Steam LED"],
  ["hardware.leds.steam.inverted", "Invert Steam LED"],

  // Hardware - Sensors
  ["hardware.sensors.temperature.type", "Temperature Sensor Type"],
  ["hardware.sensors.pressure.enabled", "Enable Pressure Sensor"],
  ["hardware.sensors.watertank.enabled", "Enable Water Tank Sensor"],
  ["hardware.sensors.watertank.mode", "Water Tank Sensor Mode"],
  ["hardware.sensors.scale.enabled", "Enable Scale"],
  ["hardware.sensors.scale.type", "Scale Setup Type"],
  ["hardware.sensors.scale.samples", "Scale Samples"],
]);

// Language maps - can be extended with other languages
export const parameterLabels = new Map<string, Map<string, string>>([
  ["en", parameterLabelsEN],
  // Add other languages here, e.g.:
  // ["de", parameterLabelsDE],
  // ["fr", parameterLabelsFR],
]);

/**
 * Get parameter label in specified language
 */
export function getParameterLabel(
  parameterName: string,
  language: string = "en",
): string {
  const langMap = parameterLabels.get(language) || parameterLabelsEN;
  return langMap.get(parameterName) || parameterName;
}

/**
 * Check if a parameter has a label in any language
 */
export function hasParameterLabel(parameterName: string): boolean {
  return parameterLabelsEN.has(parameterName);
}

/**
 * Get all available languages
 */
export function getAvailableLanguages(): string[] {
  return Array.from(parameterLabels.keys());
}

// Backward compatibility export
export default {
  en: Object.fromEntries(parameterLabelsEN),
};
