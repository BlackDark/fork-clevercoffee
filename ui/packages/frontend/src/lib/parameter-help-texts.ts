export const parameterHelpTexts: Record<string, string> = {
  "pid.enabled": "Enables or disables the PID temperature controller",
  "pid.use_ponm":
    "Use PonM mode (<a href='http://brettbeauregard.com/blog/2017/06/introducing-proportional-on-measurement/' target='_blank'>details</a>)",
  "pid.ema_factor":
    "Smoothing of input that is used for Tv (derivative component of PID). Smaller means less smoothing but also less delay, 0 means no filtering",
  "pid.regular.kp":
    "Proportional gain (in Watts/C°) for the main PID controller (in P-Tn-Tv form, <a href='http://testcon.info/EN_BspPID-Regler.html#strukturen' target='_blank'>Details<a>). The higher this value is, the higher is the output of the heater for a given temperature difference. E.g. 5°C difference will result in P*5 Watts of heater output.",
  "pid.regular.tn":
    "Integral time constant (in seconds) for the main PID controller (in P-Tn-Tv form, <a href='http://testcon.info/EN_BspPID-Regler.html#strukturen' target='_blank'>Details<a>). The larger this value is, the slower the integral part of the PID will increase (or decrease) if the process value remains above (or below) the setpoint in spite of proportional action. The smaller this value, the faster the integral term changes.",
  "pid.regular.tv":
    "Differential time constant (in seconds) for the main PID controller (in P-Tn-Tv form, <a href='http://testcon.info/EN_BspPID-Regler.html#strukturen' target='_blank'>Details<a>). This value determines how far the PID equation projects the current trend into the future. The higher the value, the greater the dampening. Select it carefully, it can cause oscillations if it is set too high or too low.",
  "pid.regular.i_max":
    "Internal integrator limit to prevent windup (in Watts). This will allow the integrator to only grow to the specified value. This should be approximally equal to the output needed to hold the temperature after the setpoint has been reached and is depending on machine type and whether the boiler is insulated or not.",
  "pid.steam.kp":
    "Proportional gain for the steaming mode (I or D are not used)",
  "brew.setpoint":
    "The temperature that the PID will attempt to reach and hold",
  "brew.temp_offset":
    "Optional offset that is added to the user-visible setpoint. Can be used to compensate sensor offsets and the average temperature loss between boiler and group so that the setpoint represents the approximate brew temperature.",
  "steam.setpoint": "The temperature that the PID will use for steam mode",
  "brew.mode":
    "Manual mode gives you full control over the brew time while Automatic mode allows you to activate brew-by-time and/or brew-by-weight. The brew will then stop at whatever target is reached first.",
  "brew.by_time.enabled":
    "Enables brew by time, so the pump stops automatically when the target brew time is reached. Only available when Brew Mode is set to Automatic",
  "brew.by_time.target_time": "Stop brew automatically after this amount of time",
  "brew.by_weight.enabled":
    "Enables brew by weight, so the pump stops automatically when the target weight is reached. Only available when Brew Mode is set to Automatic",
  "brew.by_weight.target_weight": "Brew is running until this weight has been measured",
  "brew.by_weight.auto_tare": "Automatically tare scale before brewing",
  "brew.pre_infusion.enabled":
    "Enables pre-wetting of the coffee puck by turning on the pump for a configurable length of time.",
  "brew.pre_infusion.time":
    "Time in seconds the pump is running during the pre-infusion",
  "brew.pre_infusion.pause":
    "Pause to let the puck bloom after the initial pre-infusion while turning off the pump and leaving the 3-way valve open",
  "backflush.cycles":
    "Number of cycles of filling and flushing during a backflush",
  "backflush.fill_time":
    "Time in seconds the pump is running during one backflush cycle",
  "backflush.flush_time":
    "Time in seconds the selenoid valve stays open during one backflush cycle",
  "maintenance.backflush_reminder.enabled":
    "Show a reminder on the display and web UI when the shot count since last backflush reaches the threshold. Counting continues when disabled.",
  "maintenance.backflush_reminder.threshold":
    "Number of counted brews before a backflush reminder appears (default 50 ≈ monthly at 2 shots/day). Use lower values for water-only reminders.",
  TARE_ON: "Tare the scale for zeroing before brewing.",
  CALIBRATION_ON: "Enable scale calibration mode.",
  "hardware.sensors.scale.known_weight":
    "Weight in grams of the known calibration weight used for scale setup",
  "hardware.sensors.scale.calibration":
    "Primary scale calibration factor (adjust during calibration process)",
  "hardware.sensors.scale.calibration2":
    "Secondary scale calibration factor (for dual load cell setups)",
  "pid.bd.enabled": "Use separate PID parameters while brew is running",
  "brew.pid_delay":
    "Delay time in seconds during which the PID will be disabled once a brew is detected. This prevents too high brew temperatures with boiler machines like Rancilio Silvia. Set to 0 for thermoblock machines.",
  "pid.bd.kp":
    "Proportional gain (in Watts/°C) for the PID when brewing has been detected. Use this controller to either increase heating during the brew to counter temperature drop from fresh cold water in the boiler. Some machines, e.g. Rancilio Silvia, actually need to heat less or not at all during the brew because of high temperature stability.",
  "pid.bd.tn":
    "Integral time constant (in seconds) for the PID when brewing has been detected.",
  "pid.bd.tv":
    "Differential time constant (in seconds) for the PID when brewing has been detected.",
  STEAM_MODE: "Toggle steam mode on or off.",
  BACKFLUSH_ON: "Toggle backflush mode on or off.",
  VERSION: "Firmware version (release tag or dev-YYYY-MM-DD build).",
  "standby.enabled": "Turn heater off after standby time has elapsed.",
  "standby.time":
    "Time in minutes until the heater is turned off. Timer is reset by brew, manual flush, backflush and steam.",
  "display.template": "Set the display template, changes require a reboot",
  "display.inverted": "Set the display rotation, changes require a reboot",
  "display.language":
    "Set the language for the OLED display, changes requre a reboot",
  "display.blinking.delta":
    "Delta from setpoint for status LED and blinking temperature display",
  "display.fullscreen_brew_timer": "Enable fullscreen overlay during brew",
  "display.fullscreen_manual_flush_timer":
    "Enable fullscreen overlay during manual flush",
  "display.fullscreen_hot_water_timer":
    "Enable fullscreen overlay during hot water mode",
  "display.post_brew_timer_duration":
    "time in s that brew timer will be shown after brew finished",
  "display.heating_logo":
    "full screen logo will be shown if temperature is 5°C below setpoint",
  "display.pid_off_logo": "full screen logo will be shown if pid is disabled",
  "mqtt.enabled": "Enables MQTT, change requires a restart",
  "mqtt.broker":
    "IP addresss or hostname of your MQTT broker, changes require a restart",
  "mqtt.port": "Port number of your MQTT broker, changes require a restart",
  "mqtt.username": "Username for your MQTT broker, changes require a restart",
  "mqtt.password": "Password for your MQTT broker, changes require a restart",
  "mqtt.topic": "Custom MQTT topic prefix, changes require a restart",
  "mqtt.hassio.enabled":
    "Enables Home Assistant integration, requires a restart",
  "mqtt.hassio.prefix": "Custom MQTT topic prefix, changes require a restart",
  "system.hostname": "Hostname of your machine, changes require a restart",
  "system.ota_password":
    "Password for over-the-air updates, changes require a restart",
  "system.log_level": "Set the logging verbosity level",
  "system.auth.enabled":
    "Enables authentication for accessing certain parts of the website and for web requests in general. This setting secures the calls to sensitive url endpoints, e.g. for config parameters, hardware settings, factory reset, etc.",
  "system.auth.username":
    "Username for accessing the website and authenticating web requests.",
  "system.auth.password":
    "Password for accessing the website and authenticating web requests.",
  "system.timing_debug.enabled":
    "Enable or disable the process loop time debugging in console.<br>r=draw display buffer<br>D=display refresh<br>W=website<br>M=MQTT<br>H=hassio<br>T=temperature",
  "system.showdisplay.enabled":
    "Enable or disable showing sendBuffer loops in debug logs",
  "hardware.oled.enabled": "Enable or disable the OLED display",
  "hardware.oled.type": "Select your OLED display type",
  "hardware.oled.address":
    "I2C address of the OLED display, should be 0x3C in most cases, if in doubt check the datasheet",
  "hardware.relays.heater.trigger_type":
    "Relay trigger type for heater control",
  "hardware.relays.valve.trigger_type": "Relay trigger type for valve control",
  "hardware.relays.pump.trigger_type": "Relay trigger type for pump control",
  "hardware.switches.brew.enabled": "Enable physical brew switch",
  "hardware.switches.brew.type": "Type of brew switch connected",
  "hardware.switches.brew.mode":
    "Electrical configuration of brew switch<br>Normally Open is active high<br>Normally Closed is active low",
  "hardware.switches.steam.enabled": "Enable physical steam switch",
  "hardware.switches.steam.type": "Type of steam switch connected",
  "hardware.switches.steam.mode":
    "Electrical configuration of steam switch<br>Normally Open is active high<br>Normally Closed is active low",
  "hardware.switches.power.enabled": "Enable physical power switch",
  "hardware.switches.power.type": "Type of power switch connected",
  "hardware.switches.power.mode":
    "Electrical configuration of power switch<br>Normally Open is active high<br>Normally Closed is active low",
  "hardware.switches.hot_water.enabled": "Enable physical water switch",
  "hardware.switches.hot_water.type": "Type of water switch connected",
  "hardware.switches.hot_water.mode":
    "Electrical configuration of water switch<br>Normally Open is active high<br>Normally Closed is active low",
  "hardware.leds.status.enabled": "Enable status indicator LED",
  "hardware.leds.status.inverted":
    "Invert the status LED logic (for common anode LEDs)",
  "hardware.leds.brew.enabled": "Enable brew indicator LED",
  "hardware.leds.brew.inverted":
    "Invert the brew LED logic (for common anode LEDs)",
  "hardware.leds.steam.enabled": "Enable steam indicator LED",
  "hardware.leds.steam.inverted":
    "Invert the steam LED logic (for common anode LEDs)",
  "hardware.sensors.temperature.type": "Type of temperature sensor connected",
  "hardware.sensors.pressure.enabled":
    "Enable pressure sensor for monitoring brew pressure",
  "hardware.sensors.watertank.enabled": "Enable water tank level sensor",
  "hardware.sensors.watertank.mode":
    "Electrical configuration of water tank sensor",
  "hardware.sensors.scale.enabled":
    "Enable integrated scale for weight-based brewing",
  "hardware.sensors.scale.type": "Scale load cell configuration",
  "hardware.sensors.scale.samples":
    "Number of samples to average for scale readings (higher = more stable but slower)",
};
