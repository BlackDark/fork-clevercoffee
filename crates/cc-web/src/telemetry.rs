//! [`Telemetry`] and [`Command`]: the two types every HTTP handler is a function
//! of, and the whole of the network→control surface.
//!
//! Moved out of `cc_hal_esp32::web` by finding 4.1. Neither type names
//! `esp_idf_svc`, and `Telemetry` is what `Snapshot` carries — so the payload is
//! portable even though the cell that holds it is not. See the crate root.
//!
//! # One `Telemetry`, not two
//!
//! Finding 4.5 recorded that this struct existed **twice** — here and as
//! `cc_firmware::network::Reading`, with a `telemetry_from` translation between
//! them. The translation is gone and `cc-firmware`'s copy is deleted: the control
//! task now builds a [`Telemetry`] directly. `Reading` existed because the field
//! names had drifted (`state` against `machine_state`), which is a cost paid on
//! every publish, not a separation.

/// The telemetry every handler reads.
///
/// A flat struct of `Copy` values, published into an `Arc` by the control task.
/// It is deliberately **not** `cc_machine`'s state or `cc_config`'s `Config`:
/// those are owned by tasks this one must not reach into, and a network task
/// holding a `Config` is how the C++'s `LoopManager` ↔ `MQTTManager` ↔
/// `WebServerManager` coupling happened (04 §3.2).
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "`Telemetry` is a *report of facts about the machine*, one field per \
              key the C++'s `/api/status` publishes -- and a dozen of those are \
              booleans because a dozen of the C++'s are. Turning them into \
              enums would make the payload builder unreadable and would not \
              make the data any more correct. See `the_cpp_keys_are_the_schema`."
)]
pub struct Telemetry {
    /// `MachineState` as its integer discriminant, as the C++ publishes it
    /// (`WebServerManager.cpp:349`).
    pub machine_state: i32,
    /// The boiler temperature in °C.
    pub temperature_c: f64,
    /// The brew setpoint in °C.
    pub setpoint_c: f64,
    /// The PID output in per cent (`pidOutput / 10`, `:352`).
    pub heater_power_pct: f64,
    /// Whether the PID is running.
    pub pid_enabled: bool,
    /// `MachineStateContext::steamON_` (`MachineStateContext.h:785`).
    ///
    /// Published because `/api/steam`'s toggle needs it: the C++'s handler reads
    /// `isSteamModeActive()` to compute `!current`, and this is the only copy of
    /// that fact the httpd task can reach.
    pub steam_mode: bool,
    /// `systemContext_->backflushMode()` — whether backflush *mode* is armed.
    ///
    /// Published for `/api/backflush`'s toggle, on the same reasoning as
    /// [`Self::steam_mode`].
    pub backflush_mode: bool,
    /// Whether a brew is running.
    ///
    /// **Not** the C++'s `steamMode` and not what this used to be published as.
    /// See [`crate::payload::status_json`]: `steam_mode` is that flag, this is a
    /// derived `is_brew_state()`, and the C++ publishes neither this one nor any
    /// field of this shape.
    pub brewing: bool,
    /// Whether the machine is in standby.
    pub standby: bool,
    /// Milliseconds of standby remaining.
    pub standby_remaining_ms: u32,
    /// Milliseconds since boot.
    pub uptime_ms: u32,
    /// Shots since the last backflush.
    pub shots_since_backflush: u32,
    /// `maintenance.backflush_reminder.threshold`.
    pub backflush_threshold: u32,
    /// Whether the reminder is due.
    pub backflush_due: bool,
    /// The weight in grams, if a scale is fitted and has produced a reading.
    pub weight_g: Option<f64>,
    /// The brew weight in grams.
    pub brew_weight_g: Option<f64>,
    /// The water tank float, if fitted.
    pub water_tank_full: Option<bool>,
    /// The pressure in bar, if fitted.
    pub pressure_bar: Option<f64>,
    /// The Wi-Fi signal, 0–4.
    pub signal: u8,
    /// Whether the radio is associated.
    pub wifi_associated: bool,
    /// The IPv4 address as text, or `None`.
    ///
    /// A `heapless::String<15>` -- the longest an IPv4 address in dotted-quad
    /// form can be (`255.255.255.255`) -- and **not** a `String`.
    ///
    /// This is not a style preference, and it is the field that decides whether
    /// the snapshot that carries this type is safe. It used to be a `String`,
    /// which meant the snapshot payload owned heap memory; combined with the
    /// non-atomic seqlock access that made a reader's `clone()` a
    /// use-after-free (see `cc_hal_esp32::web::Snapshot`'s docs). Fixed-size
    /// means the reader's copy is a memcpy of at most 16 bytes with no
    /// allocation, no `free`, and no allocator traffic on the httpd task.
    pub ip: Option<heapless::String<15>>,
    /// Whether MQTT is configured at all.
    pub mqtt_configured: bool,
    /// Whether MQTT has a session.
    pub mqtt_connected: bool,
    /// Whether the machine has given up on Wi-Fi.
    pub wifi_offline: bool,
}

/// A request the handlers can ask the control task to carry out.
///
/// The whole of the network→control surface for now. `Copy`, bounded, and
/// `hal::task::queue::Queue`-compatible by construction (04 §3.2: *"No
/// `String`, no `Vec`, no `Box` in a cross-task message"*).
///
/// **`POST /api/parameters` is the one command endpoint that is not in here**, and
/// the reason is that rule: a parameter write is a *list* of pairs, and a `Copy`
/// enum cannot carry a list. It travels in
/// `cc_hal_esp32::task::ParameterHandoff` instead, which is drained at the same
/// point in the tick. The write itself is not a second path —
/// [`cc_config::assign::apply`] is the only writer of a parameter, and R3-13's
/// inbound MQTT calls it directly because MQTT already runs on the control task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// `POST /api/setpoint?value=<celsius>`.
    SetSetpoint(i32),
    /// `POST /api/steam?on=0|1` — the **explicit** form.
    SetSteam(bool),
    /// `POST /api/steam` with no field — the C++'s toggle.
    ///
    /// The C++'s `/api/steam` reads no parameter at all; it computes
    /// `!isSteamModeActive()` from the **live** `MachineStateContext`
    /// (`WebServerManager.cpp:444-445`). So the decision needs the machine
    /// state, which only the control task has, and the web layer cannot make it
    /// without racing a snapshot that may be a tick stale. This variant carries
    /// no value and the control task resolves it against the machine it owns —
    /// which is the faithful translation, and the reason the toggle is a
    /// separate variant rather than a flag on [`Self::SetSteam`].
    ToggleSteam,
    /// `POST /api/pid?on=0|1` — the explicit form.
    SetPid(bool),
    /// `POST /api/pid` with no field — the C++'s toggle.
    ///
    /// `!Config::getInstance().pidEnabled.get()` (`WebServerManager.cpp:466`),
    /// which the control task both holds and writes. See [`Self::ToggleSteam`]
    /// for why the resolution happens there.
    TogglePid,
    /// `POST /api/backflush?on=0|1` — the explicit form.
    SetBackflush(bool),
    /// `POST /api/backflush` with no field — the C++'s toggle.
    ///
    /// `!systemContext_->backflushMode()` (`WebServerManager.cpp:490`). See
    /// [`Self::ToggleSteam`].
    ToggleBackflush,
    /// `POST /api/backflush?value=start` — begin a backflush cycle.
    ///
    /// **Not a C++ route.** The C++'s `/api/backflush` only toggles backflush
    /// *mode* (`WebServerManager.cpp:490`); starting a cycle is a switch press
    /// or an MQTT command. This verb exists because the previous
    /// `register_command` accepted `start` and something may already send it,
    /// and removing a reachable command would be a regression. The bare POST
    /// does **not** mean this — it means toggle, as in the C++.
    StartBackflush,
    /// `POST /api/sleep`.
    Sleep,
    /// `POST /api/wake`.
    Wake,
    /// `POST /api/scale/tare`.
    Tare,
    /// `POST /api/scale/calibration`.
    Calibrate,
    /// `POST /api/maintenance/reset-backflush-counter`.
    ResetBackflushCounter,
    /// `POST /api/wifi-reset` — forget the stored credentials and reboot.
    WifiReset,
    /// `POST /api/factory-reset` — erase the configuration and reboot.
    FactoryReset,
    /// `POST /api/restart` — reboot, keeping the configuration.
    Restart,
    /// An OTA session has started: apply `Effect::SafeHardwareShutdown`.
    ///
    /// **Not a C++ route.** The C++ calls `otaPrepareHardware()`
    /// (`SystemInitializer.cpp:57-63`) directly from the OTA module, on the main
    /// loop, because in the C++ the OTA code *is* the main loop. Here the OTA
    /// handler runs on the httpd task, which does not own the actuators — so this
    /// is requirement S8 expressed the way this firmware expresses every other
    /// web-initiated hardware action: a request the control task drains.
    ///
    /// What it buys is the applier. `otaPrepareHardware` calls
    /// `disableHeater()` on the `HardwareManager` directly; going through
    /// `Effect::SafeHardwareShutdown` reaches
    /// `Actuators::safe_hardware_shutdown` (`actuators.rs:775`), which turns the
    /// pump off and closes the valve as well as zeroing the heater duty — the
    /// difference 04 §4 names, and the one S8 asks for.
    OtaBegin,
}

#[cfg(test)]
mod tests;
