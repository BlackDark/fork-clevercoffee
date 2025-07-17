# Parameter Extraction Analysis

## Overview
This document analyzes the parameter extraction from the ESP32 backend code to the new UI system.

## Backend Analysis Summary

### Source Files Analyzed
- `src/ParameterRegistry.cpp` - Main parameter definitions
- `src/ParameterRegistry.h` - Parameter registry interface
- `src/embeddedWebserver.h` - Web API handlers

### Parameter Categories Found

#### 1. PID Parameters (Section 0)
- `pid.enabled` - Enable/disable PID controller
- `pid.use_ponm` - PonM mode setting
- `pid.ema_factor` - EMA smoothing factor
- `pid.regular.kp` - Proportional gain
- `pid.regular.tn` - Integral time constant
- `pid.regular.tv` - Differential time constant
- `pid.regular.i_max` - Integrator maximum
- `pid.steam.kp` - Steam mode proportional gain

#### 2. Temperature Control (Section 1)
- `TEMP` - Current temperature (read-only)
- `brew.setpoint` - Target brew temperature
- `brew.temp_offset` - Temperature offset
- `steam.setpoint` - Steam mode temperature

#### 3. Brew PID Parameters (Section 2)
- `pid.bd.enabled` - Enable brew detection PID
- `brew.pid_delay` - PID delay during brew
- `pid.bd.kp` - Brew detection Kp
- `pid.bd.tn` - Brew detection Tn
- `pid.bd.tv` - Brew detection Tv

#### 4. Brew Control (Section 3)
- `brew.mode` - Manual/Automatic mode
- `brew.by_time` - Enable time-based brewing
- `brew.target_time` - Target brew time
- `brew.by_weight` - Enable weight-based brewing
- `brew.target_weight` - Target brew weight
- `brew.pre_infusion.enabled` - Enable pre-infusion
- `brew.pre_infusion.time` - Pre-infusion duration
- `brew.pre_infusion.pause` - Pre-infusion pause

#### 5. Scale Parameters (Section 4)
- `hardware.sensors.scale.known_weight` - Calibration weight
- `hardware.sensors.scale.calibration` - Primary calibration factor
- `hardware.sensors.scale.calibration2` - Secondary calibration factor

#### 6. Display Settings (Section 5)
- `display.template` - Display template selection
- `display.inverted` - Display rotation
- `display.language` - Display language
- `display.fullscreen_brew_timer` - Fullscreen brew timer
- `display.fullscreen_manual_flush_timer` - Fullscreen flush timer
- `display.fullscreen_hot_water_timer` - Fullscreen hot water timer
- `display.post_brew_timer_duration` - Post-brew timer duration
- `display.heating_logo` - Show heating logo
- `display.pid_off_logo` - Show PID disabled logo

#### 7. Maintenance (Section 6)
- `backflush.cycles` - Number of backflush cycles
- `backflush.fill_time` - Backflush fill duration
- `backflush.flush_time` - Backflush flush duration

#### 8. Power Settings (Section 7)
- `standby.enabled` - Enable standby mode
- `standby.time` - Standby timeout

#### 9. MQTT Settings (Section 8)
- `mqtt.enabled` - Enable MQTT
- `mqtt.broker` - MQTT broker hostname
- `mqtt.port` - MQTT broker port
- `mqtt.username` - MQTT username
- `mqtt.password` - MQTT password
- `mqtt.topic` - MQTT topic prefix
- `mqtt.hassio.enabled` - Enable Home Assistant integration
- `mqtt.hassio.prefix` - Home Assistant topic prefix

#### 10. System Settings (Section 9)
- `system.hostname` - Device hostname
- `system.ota_password` - OTA update password
- `system.log_level` - Logging verbosity
- `system.auth.enabled` - Enable web authentication
- `system.auth.username` - Web username
- `system.auth.password` - Web password
- `system.timing_debug.enabled` - Debug timing logs
- `system.showdisplay.enabled` - Debug display logs

#### 11. Runtime Controls (Section 10)
- `STEAM_MODE` - Steam mode toggle
- `BACKFLUSH_ON` - Backflush mode toggle
- `TARE_ON` - Scale tare function
- `CALIBRATION_ON` - Scale calibration mode
- `VERSION` - Firmware version (read-only)

#### 12. Hardware Sections (Sections 11-15)

**OLED Display (Section 11):**
- `hardware.oled.enabled` - Enable OLED display
- `hardware.oled.type` - OLED controller type
- `hardware.oled.address` - I2C address

**Relays (Section 12):**
- `hardware.relays.heater.trigger_type` - Heater relay trigger
- `hardware.relays.valve.trigger_type` - Valve relay trigger
- `hardware.relays.pump.trigger_type` - Pump relay trigger

**Switches (Section 13):**
- `hardware.switches.brew.enabled` - Enable brew switch
- `hardware.switches.brew.type` - Brew switch type
- `hardware.switches.brew.mode` - Brew switch mode
- `hardware.switches.steam.enabled` - Enable steam switch
- `hardware.switches.steam.type` - Steam switch type
- `hardware.switches.steam.mode` - Steam switch mode
- `hardware.switches.power.enabled` - Enable power switch
- `hardware.switches.power.type` - Power switch type
- `hardware.switches.power.mode` - Power switch mode
- `hardware.switches.hot_water.enabled` - Enable hot water switch
- `hardware.switches.hot_water.type` - Hot water switch type
- `hardware.switches.hot_water.mode` - Hot water switch mode

**LEDs (Section 14):**
- `hardware.leds.status.enabled` - Enable status LED
- `hardware.leds.status.inverted` - Invert status LED
- `hardware.leds.brew.enabled` - Enable brew LED
- `hardware.leds.brew.inverted` - Invert brew LED
- `hardware.leds.steam.enabled` - Enable steam LED
- `hardware.leds.steam.inverted` - Invert steam LED

**Sensors (Section 15):**
- `hardware.sensors.temperature.type` - Temperature sensor type
- `hardware.sensors.pressure.enabled` - Enable pressure sensor
- `hardware.sensors.watertank.enabled` - Enable water tank sensor
- `hardware.sensors.watertank.mode` - Water tank sensor mode
- `hardware.sensors.scale.enabled` - Enable scale
- `hardware.sensors.scale.type` - Scale configuration
- `hardware.sensors.scale.samples` - Scale sample count
- `hardware.sensors.scale.calibration` - Scale calibration factor
- `hardware.sensors.scale.calibration2` - Secondary calibration factor
- `hardware.sensors.scale.known_weight` - Calibration weight

## Conditional Logic Identified

### Key Dependencies
1. **Brew Switch Dependency**: Many brew-related parameters only show when `hardware.switches.brew.enabled = 1`
2. **Scale Dependency**: Scale parameters only show when `hardware.sensors.scale.enabled = 1`
3. **MQTT Dependency**: MQTT sub-parameters only show when `mqtt.enabled = 1`
4. **Authentication Dependency**: Auth parameters only show when `system.auth.enabled = 1`
5. **Debug Dependency**: Debug parameters only show when `system.log_level = 1` (DEBUG)
6. **Hardware Dependencies**: Hardware sub-parameters only show when parent hardware is enabled

### Complex Conditions
- `brew.by_time` and `brew.target_time` only show when `brew.mode = 1` (Automatic)
- `brew.by_weight` and `brew.target_weight` only show when both `brew.mode = 1` AND `hardware.sensors.scale.enabled = 1`
- `hardware.sensors.scale.calibration2` only shows when `hardware.sensors.scale.type = 0` (2 load cells)

## Files Created/Updated

### New Files
1. `ui/data/complete_parameters.json` - Complete parameter definitions with conditions
2. `ui/src/lib/parameter-conditions.ts` - Conditional logic utilities
3. `ui/src/lib/parameter-utils.ts` - Parameter management utilities
4. `parameter-extraction-analysis.md` - This analysis document

### Updated Files
1. `ui/src/lib/parameter-groups.ts` - Updated with all parameter groups
2. `ui/src/lib/parameter-labels.ts` - Updated with all parameter labels
3. `ui/src/lib/parameter-help-texts.ts` - Ready for help text updates

## Recommendations

### 1. Implement Conditional UI Logic
The UI should implement the conditional logic to show/hide parameters based on dependencies. Use the `conditions` property in the parameter definitions.

### 2. Parameter Validation
Implement client-side validation using the min/max values and parameter types.

### 3. Help Text Integration
The help texts from the backend should be integrated into the UI system.

### 4. Section Organization
Consider reorganizing the UI to group related parameters logically, especially for hardware configuration.

### 5. Hardware Configuration Warning
Implement warnings for hardware-related parameters as they can cause dangerous behavior if misconfigured.

### 6. Real-time Updates
Consider implementing real-time parameter updates to reflect changes made by other clients or the device itself.

## Missing from Original UI
The original UI was missing several important parameters:
- Brew PID parameters
- Scale calibration parameters
- Debug logging parameters
- Several hardware configuration options
- Runtime control parameters (TARE_ON, CALIBRATION_ON)

## Next Steps
1. Implement conditional parameter display logic in the UI components
2. Add parameter validation
3. Integrate help text system
4. Test with actual hardware to verify parameter behavior
5. Add proper error handling for parameter updates
6. Implement hardware configuration warnings
