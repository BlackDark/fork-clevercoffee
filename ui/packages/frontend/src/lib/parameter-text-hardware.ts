import * as m from "@/paraglide/messages.js";

type Text = { label?: () => string; help?: () => string };

// Hardware parameter labels and help texts; merged into `texts` in parameter-text.ts.
export const hardwareTexts: Record<string, Text> = {
  "hardware.sensors.scale.known_weight": {
    label: m.param_hardware_sensors_scale_known_weight_label,
    help: m.param_hardware_sensors_scale_known_weight_help,
  },
  "hardware.sensors.scale.calibration": {
    label: m.param_hardware_sensors_scale_calibration_label,
    help: m.param_hardware_sensors_scale_calibration_help,
  },
  "hardware.sensors.scale.calibration2": {
    label: m.param_hardware_sensors_scale_calibration2_label,
    help: m.param_hardware_sensors_scale_calibration2_help,
  },
  "hardware.oled.enabled": {
    label: m.param_hardware_oled_enabled_label,
    help: m.param_hardware_oled_enabled_help,
  },
  "hardware.oled.type": {
    label: m.param_hardware_oled_type_label,
    help: m.param_hardware_oled_type_help,
  },
  "hardware.oled.address": {
    label: m.param_hardware_oled_address_label,
    help: m.param_hardware_oled_address_help,
  },
  "hardware.relays.heater.trigger_type": {
    label: m.param_hardware_relays_heater_trigger_type_label,
    help: m.param_hardware_relays_heater_trigger_type_help,
  },
  "hardware.relays.valve.trigger_type": {
    label: m.param_hardware_relays_valve_trigger_type_label,
    help: m.param_hardware_relays_valve_trigger_type_help,
  },
  "hardware.relays.pump.trigger_type": {
    label: m.param_hardware_relays_pump_trigger_type_label,
    help: m.param_hardware_relays_pump_trigger_type_help,
  },
  "hardware.switches.brew.enabled": {
    label: m.param_hardware_switches_brew_enabled_label,
    help: m.param_hardware_switches_brew_enabled_help,
  },
  "hardware.switches.brew.type": {
    label: m.param_hardware_switches_brew_type_label,
    help: m.param_hardware_switches_brew_type_help,
  },
  "hardware.switches.brew.mode": {
    label: m.param_hardware_switches_brew_mode_label,
    help: m.param_hardware_switches_brew_mode_help,
  },
  "hardware.switches.steam.enabled": {
    label: m.param_hardware_switches_steam_enabled_label,
    help: m.param_hardware_switches_steam_enabled_help,
  },
  "hardware.switches.steam.type": {
    label: m.param_hardware_switches_steam_type_label,
    help: m.param_hardware_switches_steam_type_help,
  },
  "hardware.switches.steam.mode": {
    label: m.param_hardware_switches_steam_mode_label,
    help: m.param_hardware_switches_steam_mode_help,
  },
  "hardware.switches.power.enabled": {
    label: m.param_hardware_switches_power_enabled_label,
    help: m.param_hardware_switches_power_enabled_help,
  },
  "hardware.switches.power.type": {
    label: m.param_hardware_switches_power_type_label,
    help: m.param_hardware_switches_power_type_help,
  },
  "hardware.switches.power.mode": {
    label: m.param_hardware_switches_power_mode_label,
    help: m.param_hardware_switches_power_mode_help,
  },
  "hardware.switches.hot_water.enabled": {
    label: m.param_hardware_switches_hot_water_enabled_label,
    help: m.param_hardware_switches_hot_water_enabled_help,
  },
  "hardware.switches.hot_water.type": {
    label: m.param_hardware_switches_hot_water_type_label,
    help: m.param_hardware_switches_hot_water_type_help,
  },
  "hardware.switches.hot_water.mode": {
    label: m.param_hardware_switches_hot_water_mode_label,
    help: m.param_hardware_switches_hot_water_mode_help,
  },
  "hardware.leds.status.enabled": {
    label: m.param_hardware_leds_status_enabled_label,
    help: m.param_hardware_leds_status_enabled_help,
  },
  "hardware.leds.status.inverted": {
    label: m.param_hardware_leds_status_inverted_label,
    help: m.param_hardware_leds_status_inverted_help,
  },
  "hardware.leds.brew.enabled": {
    label: m.param_hardware_leds_brew_enabled_label,
    help: m.param_hardware_leds_brew_enabled_help,
  },
  "hardware.leds.brew.inverted": {
    label: m.param_hardware_leds_brew_inverted_label,
    help: m.param_hardware_leds_brew_inverted_help,
  },
  "hardware.leds.steam.enabled": {
    label: m.param_hardware_leds_steam_enabled_label,
    help: m.param_hardware_leds_steam_enabled_help,
  },
  "hardware.leds.steam.inverted": {
    label: m.param_hardware_leds_steam_inverted_label,
    help: m.param_hardware_leds_steam_inverted_help,
  },
  "hardware.sensors.temperature.type": {
    label: m.param_hardware_sensors_temperature_type_label,
    help: m.param_hardware_sensors_temperature_type_help,
  },
  "hardware.sensors.pressure.enabled": {
    label: m.param_hardware_sensors_pressure_enabled_label,
    help: m.param_hardware_sensors_pressure_enabled_help,
  },
  "hardware.sensors.watertank.enabled": {
    label: m.param_hardware_sensors_watertank_enabled_label,
    help: m.param_hardware_sensors_watertank_enabled_help,
  },
  "hardware.sensors.watertank.mode": {
    label: m.param_hardware_sensors_watertank_mode_label,
    help: m.param_hardware_sensors_watertank_mode_help,
  },
  "hardware.sensors.watertank.keep_heater_on_empty": {
    label: m.param_hardware_sensors_watertank_keep_heater_on_empty_label,
    help: m.param_hardware_sensors_watertank_keep_heater_on_empty_help,
  },
  "hardware.sensors.scale.enabled": {
    label: m.param_hardware_sensors_scale_enabled_label,
    help: m.param_hardware_sensors_scale_enabled_help,
  },
  "hardware.sensors.scale.type": {
    label: m.param_hardware_sensors_scale_type_label,
    help: m.param_hardware_sensors_scale_type_help,
  },
  "hardware.sensors.scale.samples": {
    label: m.param_hardware_sensors_scale_samples_label,
    help: m.param_hardware_sensors_scale_samples_help,
  },
};
