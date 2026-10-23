//! Values crossing the HTTP/control boundary: [`Telemetry`] and [`Command`].
//!
//! The control task builds the shared telemetry type directly, avoiding duplicate
//! payload types and field-by-field translations that can drift.

/// The telemetry every handler reads.
///
/// A flat struct of `Copy` values, published into an `Arc` by the control task.
/// It is deliberately **not** `cc_machine`'s state or `cc_config`'s `Config`:
/// those are owned by tasks the httpd task must not reach into, and a network task
/// holding a `Config` is how this layering gets bypassed.
#[derive(Clone, Debug, Default, PartialEq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "`Telemetry` is a *report of facts about the machine*, one field per \
              key `/api/status` publishes, and a dozen of those keys are booleans \
              because a dozen of the answers are. Turning them into enums would \
              make the payload builder unreadable and would not make the data any \
              more correct."
)]
pub struct Telemetry {
    /// The machine state as its integer discriminant, as `/api/status` publishes
    /// it.
    pub machine_state: i32,
    /// The boiler temperature in °C.
    pub temperature_c: f64,
    /// The brew setpoint in °C.
    pub setpoint_c: f64,
    /// The PID output in per cent (`pid_output / 10`).
    pub heater_power_pct: f64,
    /// Whether the PID is running.
    pub pid_enabled: bool,
    /// The latched steam flag.
    ///
    /// Published because `/api/steam`'s toggle needs it: the target is `!current`,
    /// and this is the only copy of that fact the httpd task can reach.
    pub steam_mode: bool,
    /// Whether backflush *mode* is armed.
    ///
    /// Published for `/api/backflush`'s toggle, for the same reason as
    /// [`Self::steam_mode`].
    pub backflush_mode: bool,
    /// Whether a brew is running.
    ///
    /// A different fact from [`Self::steam_mode`], not another name for it; see
    /// [`crate::web::payload::status_json`].
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
    /// the snapshot that carries this type is safe. A `String` here would make the
    /// snapshot payload own heap memory, and the snapshot's access is not atomic,
    /// so a reader's `clone()` would be a use-after-free (see
    /// `cc_net_esp32::web::Snapshot`). Fixed-size means the reader's copy is a
    /// memcpy of at most 16 bytes: no allocation, no `free`, and no allocator
    /// traffic on the httpd task.
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
/// The whole of the network→control surface. `Copy`, bounded, and
/// `hal::task::queue::Queue`-compatible by construction: no `String`, no `Vec`,
/// no `Box` crosses a task boundary in a message.
///
/// **`POST /api/parameters` is the one command endpoint that is not in here**, and
/// the reason is that rule: a parameter write is a *list* of pairs, and a `Copy`
/// enum cannot carry a list. It travels in
/// `cc_hal_esp32::task::ParameterHandoff` instead, which is drained at the same
/// point in the tick. The write itself is not a second path —
/// [`cc_config::assign::apply`] is the only writer of a parameter, and inbound MQTT
/// calls it directly because MQTT already runs on the control task.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    /// `POST /api/setpoint?value=<celsius>` — an `f64`, not an `i32`.
    ///
    /// `brew.setpoint` is a float parameter and the machine carries it as an `f64`
    /// end to end, so a fractional setpoint is meaningful all the way to the PID.
    SetSetpoint(f64),
    /// `POST /api/steam?on=0|1` — the **explicit** form.
    SetSteam(bool),
    /// `POST /api/steam` with no field — the toggle.
    ///
    /// Deciding the target needs the **live** machine flag, which only the control
    /// task has; the web layer cannot compute it without racing a snapshot that may
    /// be a tick stale. So this variant carries no value and the control task
    /// resolves it against the machine it owns, which is why the toggle is a
    /// separate variant rather than a flag on [`Self::SetSteam`].
    ToggleSteam,
    /// `POST /api/pid?on=0|1` — the explicit form.
    SetPid(bool),
    /// `POST /api/pid` with no field — the toggle.
    ///
    /// The control task both holds and writes the PID flag, so it resolves this
    /// one; see [`Self::ToggleSteam`].
    TogglePid,
    /// `POST /api/backflush?on=0|1` — the explicit form.
    SetBackflush(bool),
    /// `POST /api/backflush` with no field — the toggle, resolved by the control
    /// task. See [`Self::ToggleSteam`].
    ToggleBackflush,
    /// `POST /api/backflush?value=start` — begin a backflush cycle.
    ///
    /// A bare `POST /api/backflush` does **not** mean this; it means toggle. This
    /// verb exists because `start` was accepted here before and something may
    /// already send it, and removing a reachable command would be a regression.
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
    /// The OTA handler runs on the httpd task, which does not own the actuators,
    /// so this is requirement S8 expressed the way every other web-initiated
    /// hardware action is: a request the control task drains.
    ///
    /// What it buys is the applier. Zeroing the heater duty alone would leave the
    /// pump and the valve as they were; `Effect::SafeHardwareShutdown` reaches
    /// `Actuators::safe_hardware_shutdown`, which turns the pump off and closes the
    /// valve as well. That is the difference S8 asks for.
    OtaBegin,
}

#[cfg(test)]
mod tests;
