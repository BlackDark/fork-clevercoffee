import { parameterLabelsEN } from "./parameter-labels";

/**
 * Parameters whose changes only take effect after a device restart.
 * Mirrors upstream CleverCoffee ParameterRegistry reboot flags.
 */
export const REBOOT_REQUIRED_PARAMETERS = new Set<string>([
  "display.template",
  "display.inverted",
  "display.language",
  "mqtt.enabled",
  "mqtt.broker",
  "mqtt.port",
  "mqtt.username",
  "mqtt.password",
  "mqtt.topic",
  "mqtt.hassio.enabled",
  "mqtt.hassio.prefix",
  "system.hostname",
  "system.ota_password",
  "system.showdisplay.enabled",
  "hardware.oled.enabled",
  "hardware.oled.type",
  "hardware.oled.address",
  "hardware.relays.heater.trigger_type",
  "hardware.relays.valve.trigger_type",
  "hardware.relays.pump.trigger_type",
  "hardware.switches.brew.enabled",
  "hardware.switches.brew.type",
  "hardware.switches.brew.mode",
  "hardware.switches.steam.enabled",
  "hardware.switches.steam.type",
  "hardware.switches.steam.mode",
  "hardware.switches.power.enabled",
  "hardware.switches.power.type",
  "hardware.switches.power.mode",
  "hardware.switches.hot_water.enabled",
  "hardware.switches.hot_water.type",
  "hardware.switches.hot_water.mode",
  "hardware.leds.status.enabled",
  "hardware.leds.status.inverted",
  "hardware.leds.brew.enabled",
  "hardware.leds.brew.inverted",
  "hardware.leds.steam.enabled",
  "hardware.leds.steam.inverted",
  "hardware.sensors.temperature.type",
  "hardware.sensors.pressure.enabled",
  "hardware.sensors.watertank.enabled",
  "hardware.sensors.watertank.mode",
  "hardware.sensors.scale.enabled",
  "hardware.sensors.scale.type",
  "hardware.sensors.scale.samples",
]);

export function parameterRequiresReboot(name: string): boolean {
  return REBOOT_REQUIRED_PARAMETERS.has(name);
}

export function getRebootParameterLabel(name: string): string {
  return parameterLabelsEN.get(name) ?? name;
}

export function getRebootParameterLabels(names: string[]): string[] {
  return names.map(getRebootParameterLabel);
}
