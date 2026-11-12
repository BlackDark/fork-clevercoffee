import * as m from "@/paraglide/messages.js";

// Enum option labels the UI owns. Firmware labels stay the wire vocabulary
// (MQTT commands, provisioning) and only show where no entry exists here.
const optionTexts: Record<string, Record<number, () => string>> = {
  "brew.mode": { 0: m.status_manual, 1: m.status_automatic },
  "display.template": {
    0: m.status_standard,
    1: m.status_minimal,
    2: m.status_temp_only,
    3: m.status_scale,
    4: m.status_upright,
    5: m.status_modern,
  },
  // Endonyms, like the device's own names for them.
  "display.language": {
    0: m.status_english,
    1: () => "Deutsch",
    2: () => "Español",
  },
  "hardware.sensors.scale.type": {
    0: m.status_2_load_cells,
    1: m.status_1_load_cell,
  },
  "hardware.relays.heater.trigger_type": {
    0: m.status_low_trigger,
    1: m.status_high_trigger,
  },
  "hardware.relays.valve.trigger_type": {
    0: m.status_low_trigger,
    1: m.status_high_trigger,
  },
  "hardware.relays.pump.trigger_type": {
    0: m.status_low_trigger,
    1: m.status_high_trigger,
  },
  "hardware.switches.brew.type": { 0: m.status_momentary, 1: m.status_toggle },
  "hardware.switches.steam.type": { 0: m.status_momentary, 1: m.status_toggle },
  "hardware.switches.power.type": { 0: m.status_momentary, 1: m.status_toggle },
  "hardware.switches.hot_water.type": {
    0: m.status_momentary,
    1: m.status_toggle,
  },
  "hardware.switches.brew.mode": {
    0: m.status_normally_open,
    1: m.status_normally_closed,
  },
  "hardware.switches.steam.mode": {
    0: m.status_normally_open,
    1: m.status_normally_closed,
  },
  "hardware.switches.power.mode": {
    0: m.status_normally_open,
    1: m.status_normally_closed,
  },
  "hardware.switches.hot_water.mode": {
    0: m.status_normally_open,
    1: m.status_normally_closed,
  },
  "hardware.sensors.watertank.mode": {
    0: m.status_normally_open,
    1: m.status_normally_closed,
  },
};

export function getOptionLabel(
  name: string,
  value: number,
): string | undefined {
  return optionTexts[name]?.[value]?.();
}
