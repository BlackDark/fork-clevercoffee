//! The control task: sole subscriber and feeder of the task watchdog, the only task
//! that may open the heater gate or write an actuator pin, and the only owner of the
//! configuration store.
//!
//! It owns [`cc_hal_esp32::Actuators`] and [`Control`], so [`ControlLoop::tick`] is
//! the only path from a sensor reading to a relay, and `cc_machine::apply` the only
//! function in it that writes hardware.
//!
//! # The tick, in order
//!
//! The order is the safety property, and [`ControlLoop::tick`] is the list:
//!
//! 1. **Feed the watchdog**, first: feeding last would report an overrun as a hang.
//! 2. **Drain** the command queue and one parameter write, through the reducer, so a
//!    request cannot reach past the tick into control state.
//! 3. **Take a staged Wi-Fi credential** from the console.
//! 4. **Sense**: temperature, switches, pressure, tank, weight.
//! 5. **Beat the heater's deadman**, on the same signal as the watchdog feed.
//! 6. **Decide** (`Control::tick`) and 7. **act** (`cc_machine::apply`), then the OTA
//!    shutdown in its own pass so nothing in the tick can re-energise after it.
//! 8. **Show and notify**: LEDs, frame, reboots, telemetry, radio, MQTT, SSE, logs.
//! 9. **Sleep** for the rest of the period.

use std::sync::Arc;

use cc_domain::state::MachineState;
use cc_domain::units::{Celsius, Millis};
use cc_hal_esp32::display_shared::SharedBus;
use cc_hal_esp32::time::now_ms;
use cc_machine::{Effect, Effects, Event, MachineChannels};
use cc_web::Telemetry;
use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::task::watchdog::{TWDTConfig, TWDTDriver, WatchdogSubscription, TWDT};
use log::{debug, error, info, warn};

use crate::config_io::{
    drain_scale, persist_config, persist_pid_enabled, persist_setpoint, push_into_machine,
};
use crate::control::Control;
use crate::probe::{DallasFaultTag, LastReading, TemperatureSensor};
use crate::{display_task, mqtt_link, network, slots, EspError};

/// The control task's period, in milliseconds — 100 Hz.
///
/// The safety cadence, so nothing slower may set it. Each sensor runs on its own
/// cadence inside it; the panel refreshes at [`display_task::REFRESH_MS`].
pub(crate) const CONTROL_PERIOD_MS: u32 = 10;

/// The deadman gate's beat period, and so the bound on how long the heater stays
/// energised after the control task stops.
///
/// The beat is the real clock, not a tick count: a late tick is the case the
/// deadman exists to catch. The watchdog feed has the same period on purpose —
/// one signal that the task is alive, not two that could disagree.
pub(crate) const HEARTBEAT_MS: u32 = CONTROL_PERIOD_MS;

// One interlock period of headroom: a tick period at or above the deadman timeout
// would let a single late tick drop the heater.
const _: () = assert!(
    HEARTBEAT_MS * 2 <= cc_domain::heater::DEADMAN_TIMEOUT_MS,
    "the control tick must be at most half the deadman timeout, or one late \
     tick drops the heater"
);

/// How often the published parameter snapshot is rebuilt.
///
/// Not [`HEARTBEAT_MS`]: `parameters_json` costs ~420 allocations and 20.5 KB per
/// call, 42,000 allocations a second at the tick rate. 1 s is the UI's poll rate.
const PARAMETERS_PUBLISH_MS: u32 = 1_000;

/// How often a frame is handed to the display task: the panel's own refresh.
const FRAME_PUBLISH_MS: u32 = display_task::REFRESH_MS;

/// The tick's work budget, as opposed to its period.
const TICK_BUDGET_MS: u32 = 10;

/// Ticks in the pre-scale baseline: 10 s, past first-conversion settling.
const TICK_BASELINE_TICKS: u32 = 1_000;

/// How often the tick-timing report is logged.
const TICK_REPORT_INTERVAL_MS: u32 = 30_000;

/// The SSE and history cadence. The `new_temps` frame goes to every client.
const SSE_INTERVAL_MS: u32 = 1_000;

/// How often the heap is logged. An OOM crashes faster than this; it is for the
/// operator, not the machine.
const HEAP_LOG_INTERVAL_MS: u32 = 60_000;

/// The radio's maintenance cadence, owned by the Wi-Fi monitor.
const WIFI_POLL_MS: u32 = cc_hal_esp32::wifi::MONITOR_PERIOD_MS;

/// The pause between the last console line and a reboot, so UART0 drains and the
/// operator sees why. Measured: without it `just wifi-provision` missed the
/// confirmation of a credential that had been stored.
const REBOOT_DISPLAY_MS: u32 = 1_000;

/// The pause before restarting into a new OTA image, so the `200` reaches the client.
const OTA_RESTART_DELAY_MS: u32 = 1_000;

/// The pause before an HTTP-requested restart, so the `202` leaves the socket.
const HTTP_RESTART_DELAY_MS: u32 = 500;

/// The heartbeat and PID log cadence. One line per tick costs ~13 ms of UART at
/// 115200 baud, more than the tick's whole budget.
const LOG_INTERVAL_MS: u32 = 1_000;

/// Everything the control task owns, moved in from `bring_up`.
///
/// **Boxed** by the caller: `Config` alone is 632 bytes, and a by-value argument is
/// materialised in the caller's frame *and* the callee's. Two copies overflowed the
/// control stack on hardware and faulted four steps into the tick.
pub(crate) struct ControlArgs {
    /// The only owner of the pump, the valve and the heater.
    pub(crate) actuators: cc_hal_esp32::Actuators,
    /// The status and brew LEDs; this task is their only writer.
    pub(crate) leds: cc_hal_esp32::leds::Leds,
    /// The five operator inputs, debounced.
    pub(crate) switches: cc_hal_esp32::SwitchBank,
    /// The leaked I²C bus, lent to the ABP2. The panel holds the same reference.
    pub(crate) shared_i2c: Option<&'static SharedBus>,
    /// The temperature probe. Must stay on this task: a second task polling it
    /// asserts (see [`crate::sensor_task`]).
    pub(crate) temp: TemperatureSensor,
    /// The shared HTTP/MQTT telemetry slot.
    pub(crate) net: Arc<network::Network>,
    /// The bounded network-to-control command queue.
    pub(crate) commands: Arc<cc_hal_esp32::task::CommandQueue>,
    /// The frame hand-off to the display task.
    pub(crate) frame: Arc<slots::FrameSlot>,
    /// The parameter-write mailbox. Separate from `commands` because a write is a
    /// list of pairs, which a `Copy` command cannot carry.
    pub(crate) parameters: Arc<cc_hal_esp32::task::ParameterHandoff>,
    /// The authoritative configuration. The HTTP server's boot copy can go stale;
    /// this one cannot.
    pub(crate) config: cc_config::Config,
    /// Whether a broker is configured. The client itself is built in this task.
    pub(crate) mqtt_configured: bool,
    /// The configuration store, moved: `load` and `save` take `&mut self`.
    pub(crate) store: cc_config::blob_store::BlobConfigStore<cc_hal_esp32::nvs::EspNvsBlob>,
    /// The scale's sampling task.
    pub(crate) sampler: Option<cc_hal_esp32::Sampler>,
    /// The UART provisioning handoff.
    pub(crate) handoff: network::Handoff,
    /// The radio, polled here because this task's stalls are already fatal.
    pub(crate) sta: Option<cc_hal_esp32::Sta>,
}

// The size is the whole argument for the `Box`; make growth a decision.
const _: () = assert!(
    std::mem::size_of::<ControlArgs>() < 2048,
    "ControlArgs has outgrown 2 KB; see its documentation before raising this"
);

/// Run the control task until the chip resets.
///
/// # Errors
///
/// Only from the watchdog: a subscription or a feed that fails.
pub(crate) fn run(twdt: TWDT<'static>, args: Box<ControlArgs>) -> Result<(), EspError> {
    // `TWDTConfig::new()` takes the timeout and panic behaviour from kconfig.
    let wdt_config = TWDTConfig::new();
    info!(
        "control task: watchdog timeout {:?}, panic_on_trigger {}",
        wdt_config.duration, wdt_config.panic_on_trigger
    );
    let mut driver = TWDTDriver::new(twdt, &wdt_config)?;
    let mut watchdog = driver.watch_current_task()?;
    info!(
        "control task: esp_task_wdt_add -> 0 (subscribed); tick and deadman beat \
         every {HEARTBEAT_MS} ms, deadman {} ms",
        cc_domain::heater::DEADMAN_TIMEOUT_MS
    );

    let mut control = ControlLoop::boot(args);
    loop {
        control.tick(&mut watchdog)?;
    }
}

/// When each slower-than-the-tick job last ran, in `now_ms` time.
#[derive(Default)]
struct Cadence {
    parameters: u32,
    frame: u32,
    radio: u32,
    sse: u32,
    heap_log: u32,
    heartbeat_log: u32,
    pid_log: u32,
}

/// The tick-timing instrument: the evidence for "the control tick is unaffected".
///
/// The **worst** tick is the number that matters, against a 10 ms budget, reported
/// in two phases: the first [`TICK_BASELINE_TICKS`] before the scale is doing
/// anything, then the rest. The means are per report window; dividing a window's
/// totals by the cumulative count once printed "8 ms" for a loop doing 3 ms.
///
/// Remeasured 2026-10-09: mean work 1 ms, period 10 ms, worst 62 ms during boot,
/// ~5 % of later ticks over 10 ms. Do not add per-section timers.
struct TickTiming {
    worst_ms: u32,
    baseline_worst_ms: u32,
    over_budget: u32,
    last_report_ms: u32,
    work_total_ms: u64,
    period_total_ms: u64,
    ticks_in_window: u32,
    last_began_ms: u32,
}

impl TickTiming {
    fn new(now: u32) -> Self {
        Self {
            worst_ms: 0,
            baseline_worst_ms: 0,
            over_budget: 0,
            last_report_ms: 0,
            work_total_ms: 0,
            period_total_ms: 0,
            ticks_in_window: 0,
            last_began_ms: now,
        }
    }

    /// Record one tick's work, measured **before** the sleep: the sleep is the
    /// period, and including it made every tick look like a 40x overrun.
    fn record(&mut self, tick: u32, began_ms: u32, elapsed_ms: u32) {
        self.worst_ms = self.worst_ms.max(elapsed_ms);
        self.work_total_ms = self.work_total_ms.saturating_add(u64::from(elapsed_ms));
        self.ticks_in_window = self.ticks_in_window.saturating_add(1);
        self.period_total_ms = self
            .period_total_ms
            .saturating_add(u64::from(began_ms.wrapping_sub(self.last_began_ms)));
        self.last_began_ms = began_ms;
        if tick <= TICK_BASELINE_TICKS {
            self.baseline_worst_ms = self.baseline_worst_ms.max(elapsed_ms);
        } else if elapsed_ms > TICK_BUDGET_MS {
            self.over_budget += 1;
        }
    }

    /// The window's mean work and achieved period, resetting the window, when a
    /// report is due.
    fn take_report(&mut self, began_ms: u32) -> Option<(u32, u32)> {
        if began_ms.wrapping_sub(self.last_report_ms) < TICK_REPORT_INTERVAL_MS {
            return None;
        }
        self.last_report_ms = began_ms;
        let ticks = u64::from(self.ticks_in_window.max(1));
        let means = (
            u32::try_from(self.work_total_ms / ticks).unwrap_or(0),
            u32::try_from(self.period_total_ms / ticks).unwrap_or(0),
        );
        self.work_total_ms = 0;
        self.period_total_ms = 0;
        self.ticks_in_window = 0;
        Some(means)
    }
}

/// What one tick read, at step 4.
struct Reading {
    /// The probe's cached reading and whether it was plausible.
    temperature: LastReading,
    /// This tick's ABP2 poll, when it produced a sample.
    pressure_bar: Option<f64>,
    tank_full: bool,
    edges: cc_hal_esp32::switches::Edges,
    /// The load cell's weight. A reading, not an event: see [`scale_weight`].
    weight_g: Option<f64>,
}

impl Reading {
    fn celsius_or(&self, default: f64) -> f64 {
        self.temperature.map_or(default, |(celsius, _)| celsius)
    }
}

/// The control task's state between ticks.
pub(crate) struct ControlLoop {
    actuators: cc_hal_esp32::Actuators,
    leds: cc_hal_esp32::leds::Leds,
    switches: cc_hal_esp32::SwitchBank,
    temp: TemperatureSensor,
    /// The ABP2, a peer of the panel on the shared bus: neither holds it across a
    /// transaction.
    pressure: Option<cc_hal_esp32::Abp2Pressure<&'static SharedBus>>,
    net: Arc<network::Network>,
    commands: Arc<cc_hal_esp32::task::CommandQueue>,
    frame: Arc<slots::FrameSlot>,
    parameters: Arc<cc_hal_esp32::task::ParameterHandoff>,
    config: cc_config::Config,
    mqtt_configured: bool,
    store: cc_config::blob_store::BlobConfigStore<cc_hal_esp32::nvs::EspNvsBlob>,
    sampler: Option<cc_hal_esp32::Sampler>,
    handoff: network::Handoff,
    sta: Option<cc_hal_esp32::Sta>,
    control: Control,
    side: cc_hal_esp32::FirmwareSide,
    mqtt: Option<mqtt_link::Link>,
    /// `TARE_ON` / `CALIBRATION_ON` latches. See [`mqtt_link::ScaleModes`].
    scale_modes: mqtt_link::ScaleModes,
    /// Carried between frames: `brew_timer` is the brew-timer FSM's state, and a
    /// value rebuilt from defaults could never leave `Idle`.
    display_input: cc_display::model::DisplayInput,
    /// The last sensor fault logged, so a missing probe says so once.
    sensor_fault_logged: Option<DallasFaultTag>,
    /// An admitted OTA session: hardware held off until it ends.
    ota_admitted: bool,
    tick: u32,
    cadence: Cadence,
    timing: TickTiming,
}

impl ControlLoop {
    /// Restore the shot counter, boot the machine and apply its boot effects.
    fn boot(args: Box<ControlArgs>) -> Box<Self> {
        let ControlArgs {
            mut actuators,
            leds,
            mut switches,
            shared_i2c,
            temp,
            net,
            commands,
            frame,
            parameters,
            config,
            mqtt_configured,
            store,
            sampler,
            handoff,
            sta,
        } = *args;

        // The machine is born holding the count rather than told it afterwards. A
        // failed read is a reminder that fires early, never a refusal to boot.
        let restored_shots = match cc_hal_esp32::nvs::load_shots_since_backflush(store.backend()) {
            Ok(Some(shots)) => {
                info!("maintenance: loaded {shots} shots since backflush");
                shots
            }
            Ok(None) => {
                info!("maintenance: no stored shot count — starting at 0");
                0
            }
            Err(err) => {
                warn!(
                    "maintenance: the stored shot count could not be read ({err}) — starting at 0"
                );
                0
            }
        };
        // Seeded so the first tick does not re-write a count that is already stored.
        let mut side = cc_hal_esp32::FirmwareSide::with_shots_since_backflush(Some(restored_shots));

        let pressure =
            shared_i2c.and_then(|bus| crate::bring_up::bring_up_pressure(Some(bus), &config));

        // The first poll is a dead read: `Debounced` seeds "not pressed" and accepts
        // a change only after 20 ms, so a toggle power switch held on still boots to
        // `PID_DISABLED` and the next ticks settle it.
        let boot_now = Millis::new(now_ms());
        let _ = switches.poll(boot_now);
        let power_pressed = config
            .hardware
            .switches
            .power
            .enabled
            .then_some(switches.levels().power);
        crate::bring_up::log_switch_levels(switches.levels());
        let (control, boot_effects) =
            Control::boot(&config, boot_now, power_pressed, restored_shots);
        // Boot effects take the same path as every tick's; the facade is told the
        // clock first because its methods take none.
        actuators.set_now(boot_now);
        actuators.set_state(control.state());
        actuators.set_water_tank_full(switches.water_tank_full());
        actuators.set_latched(control.safety_state().latched);
        cc_machine::apply(&mut actuators, &mut side, control.machine(), &boot_effects);

        // Built here, not in `bring_up`: this task holds the config, the store and the
        // machine the client turns into traffic. A failure leaves the machine running
        // without MQTT, and `/api/status` says so.
        let mqtt = if mqtt_configured {
            mqtt_link::Link::new(&config)
                .map_err(|err| {
                    warn!("mqtt: the client did not start: {err:?} — the machine runs without it");
                })
                .ok()
        } else {
            None
        };

        Box::new(Self {
            actuators,
            leds,
            switches,
            temp,
            pressure,
            net,
            commands,
            frame,
            parameters,
            config,
            mqtt_configured,
            store,
            sampler,
            handoff,
            sta,
            control,
            side,
            mqtt,
            scale_modes: mqtt_link::ScaleModes::default(),
            display_input: cc_display::model::DisplayInput::default(),
            sensor_fault_logged: None,
            ota_admitted: false,
            tick: 0,
            cadence: Cadence::default(),
            timing: TickTiming::new(now_ms()),
        })
    }

    /// One control period. See the module docs for why the order is the contract.
    fn tick(&mut self, watchdog: &mut WatchdogSubscription<'_>) -> Result<(), EspError> {
        // Taken right after the last sleep, so the measurement excludes it.
        let began_ms = now_ms();
        if self.ota_admitted && !self.net.ota.is_busy() && !self.net.ota.restart_pending() {
            self.ota_admitted = false;
            self.actuators.release_shutdown_announcement();
        }
        watchdog.feed()?;
        self.tick = self.tick.wrapping_add(1);
        let now = Millis::new(began_ms);

        // `Effects`, not `Vec`: filled four to five times per 10 ms tick, and a heap
        // allocation here sat in the loop that runs the heater deadman.
        let mut effects = Effects::new();
        let (commands_applied, ota_requested) = self.drain_commands(&mut effects);
        self.apply_parameter_write(&mut effects);
        self.take_console_credential();

        let reading = self.sense(now);

        // The gate is opened only by this line, and only this task holds a
        // `&mut HeaterGate`: "the supervisor is running" is a precondition of heating.
        self.actuators.gate().heartbeat(now);
        // The interlocks' inputs before the effects, so a duty emitted this tick is
        // judged against this tick's facts. The state comes after the tick.
        self.actuators.set_now(now);
        self.actuators.set_water_tank_full(reading.tank_full);
        self.actuators
            .set_latched(self.control.safety_state().latched);

        let sensors = self.sensors(&reading);
        let tick_effects = self
            .control
            .tick(&self.config, sensors, &reading.edges, now);
        effects.extend(&tick_effects);
        // The state this tick's transition produced: `may_open_water` is a whitelist
        // over it, so entering `BREW_PREINFUSION` would otherwise be refused for a tick.
        self.actuators.set_state(self.control.state());
        cc_machine::apply(
            &mut self.actuators,
            &mut self.side,
            self.control.machine(),
            &effects,
        );
        self.admit_or_hold_ota(ota_requested);
        self.persist_shots();
        self.show_leds(&reading, began_ms);

        if now_ms().wrapping_sub(self.cadence.parameters) >= PARAMETERS_PUBLISH_MS {
            self.cadence.parameters = now_ms();
            self.parameters
                .publish_live(cc_web::parameters_json(&self.config));
        }
        // The scale's events. Its NVS commit must not sit between the reducer's
        // decision and the pins, so it is here, after them.
        drain_scale(
            self.sampler.as_ref(),
            &mut self.store,
            &mut self.scale_modes,
        );

        // `/api/status` and the display widget report this one value.
        let backflush_due = cc_machine::maintenance::is_reminder_due(
            self.control.machine().shots_since_backflush,
            self.config.maintenance.backflush_reminder.enabled,
            self.config.maintenance.backflush_reminder.threshold,
        );
        self.publish_frame(&reading, began_ms, backflush_due);
        self.take_reboot_requests();

        let uptime = now_ms();
        self.publish_telemetry(&reading, uptime, backflush_due);
        // Acknowledge only now: a waiting `POST` re-reads the snapshot on wake, and
        // waking it before the publish answered with the previous tick's state.
        for _ in 0..commands_applied {
            self.net.shared.note_applied();
        }
        self.service_radio(uptime);
        self.service_mqtt(&reading, uptime, backflush_due);
        self.service_history(&reading, uptime);
        if uptime.wrapping_sub(self.cadence.heap_log) >= HEAP_LOG_INTERVAL_MS {
            self.cadence.heap_log = uptime;
            network::log_heap_once_a_minute(&self.net);
        }
        self.log_heartbeat(now, began_ms);
        self.log_pid(&sensors, reading.tank_full, began_ms);

        self.timing
            .record(self.tick, began_ms, now_ms().wrapping_sub(began_ms));
        if let Some(means) = self.timing.take_report(began_ms) {
            self.log_timing(means);
        }

        // Sleep the remainder only, saturating after an overrun so a wrapped deadline
        // is not a 49-day sleep. Blocking wake channels tripped kernel assertions on
        // this build; see `sensor_task.rs`.
        let elapsed = now_ms().wrapping_sub(began_ms);
        FreeRtos::delay_ms(CONTROL_PERIOD_MS.saturating_sub(elapsed));

        // Arm the chopper after the first beat, so the first duty the ISR sees went
        // through the gate.
        if self.tick == 1 {
            self.actuators.arm_heater_isr();
            info!("heater: 10 ms ISR armed after the first supervisor beat");
        }
        Ok(())
    }

    /// Step 2: every queued command, as reducer events where it is one.
    ///
    /// Returns how many were drained, to acknowledge after the telemetry publish,
    /// and whether an OTA session was requested.
    fn drain_commands(&mut self, effects: &mut Effects) -> (u32, bool) {
        let mut applied = 0;
        let mut ota_requested = false;
        while let Some(command) = self.commands.recv() {
            info!("control: command {command:?}");
            applied += 1;
            ota_requested |= self.handle_command(command, effects);
        }
        (applied, ota_requested)
    }

    /// Fold one command. Returns `true` for an OTA request, which is admitted only
    /// after this tick's effects are applied (see [`Self::admit_or_hold_ota`]).
    fn handle_command(
        &mut self,
        command: cc_hal_esp32::web::Command,
        effects: &mut Effects,
    ) -> bool {
        use cc_hal_esp32::web::Command as Web;
        use cc_machine::Command as Machine;

        match command {
            Web::Restart => self.net.shared.set_reboot_requested(),
            Web::OtaBegin => {
                info!("control: OTA session requested — admission re-checked after apply");
                return true;
            }
            // The scale commands reach the sampling task, which owns the pins.
            Web::Tare => match self.sampler.as_ref() {
                Some(sampler) if sampler.request_tare() => info!("scale: tare requested"),
                Some(_) => warn!("scale: the tare request was dropped — the sampler is behind"),
                None => warn!("scale: tare requested with no scale fitted"),
            },
            Web::Calibrate => {
                let known = self.config.hardware.sensors.scale.known_weight;
                match self.sampler.as_ref() {
                    Some(sampler) if sampler.request_calibrate(known) => {
                        info!("scale: calibration requested against {known} g");
                    }
                    Some(_) => {
                        warn!("scale: the calibration request was dropped — the sampler is behind");
                    }
                    None => warn!("scale: calibration requested with no scale fitted"),
                }
            }
            Web::SetSetpoint(celsius) => self.set_setpoint(celsius, effects),
            // Persist the preference and set the runtime flag.
            Web::SetPid(enabled) => self.set_pid(enabled, effects),
            // The negation happens here, against the machine this task owns: the web
            // task cannot read it without racing. Negate the operator's setting
            // (`runtime_enabled`), not the derived gate, which `process_control`
            // rewrites every tick and would make the toggle stick in `PID_DISABLED`.
            Web::TogglePid => {
                let enabled = !self.control.machine().pid.runtime_enabled;
                self.set_pid(enabled, effects);
                self.feed(Machine::NormalOperation, effects);
                info!("config: POST /api/pid toggled pid.enabled -> {enabled}");
            }
            Web::ToggleSteam => {
                let on = !self.control.machine().steam_mode;
                self.feed(steam_request(on), effects);
                self.feed(Machine::NormalOperation, effects);
                info!("config: POST /api/steam toggled steam mode -> {on}");
            }
            // One command carrying the value: `BackflushStop` stops a cycle and leaves
            // `backflush.on` set, so an either/or mapping cannot turn the mode off.
            Web::ToggleBackflush => {
                let on = !self.control.machine().backflush.on;
                self.feed(Machine::SetBackflushMode(on), effects);
                self.feed(Machine::NormalOperation, effects);
                info!("config: POST /api/backflush toggled backflush mode -> {on}");
            }
            Web::SetSteam(on) => self.feed(steam_request(on), effects),
            Web::SetBackflush(on) => self.feed(Machine::SetBackflushMode(on), effects),
            Web::StartBackflush => self.feed(Machine::BackflushCycleStart, effects),
            Web::Sleep => self.feed(Machine::Standby, effects),
            Web::Wake => self.feed(Machine::NormalOperation, effects),
            // The same method the reducer's `ResetShotsSinceBackflush` calls, so the
            // NVS write is one path: two would let the count come back on reboot.
            Web::ResetBackflushCounter => {
                self.control.reset_shots_since_backflush();
                self.side.on_reset_shots_since_backflush(0);
            }
            Web::WifiReset => warn!("control: POST /api/wifi-reset is not wired into this build"),
            Web::FactoryReset => {
                warn!("control: POST /api/factory-reset is not wired into this build");
            }
        }
        false
    }

    fn feed(&mut self, command: cc_machine::Command, effects: &mut Effects) {
        self.control
            .feed(&self.config, Event::Command(command), effects);
    }

    /// Set, wake and persist `brew.setpoint`, unless the result would be unsafe.
    ///
    /// The handler already parsed the value against the schema range; this is the
    /// cross-parameter half (`emergency_temp` against setpoint plus hysteresis).
    fn set_setpoint(&mut self, celsius: f64, effects: &mut Effects) {
        let mut candidate = self.config.clone();
        candidate.brew.setpoint = celsius;
        if let Err(violation) = cc_safety::validate_config(&cc_machine::safety_config(&candidate)) {
            let keys = violation.implicated_keys();
            error!(
                "config: REFUSED — brew.setpoint = {celsius} would leave the \
                 configuration unsafe ({violation:?}); implicated key(s): \
                 {keys:?}. Nothing was written and nothing was changed."
            );
            return;
        }
        self.config.brew.setpoint = celsius;
        self.control.set_setpoint(celsius);
        persist_setpoint(&mut self.store, celsius, &self.config);
        self.feed(cc_machine::Command::NormalOperation, effects);
    }

    /// Persist `pid.enabled` and set the runtime flag.
    fn set_pid(&mut self, enabled: bool, effects: &mut Effects) {
        self.config.pid.enabled = enabled;
        persist_pid_enabled(&mut self.store, enabled, &self.config);
        self.feed(cc_machine::Command::SetUserPidEnabled(enabled), effects);
    }

    /// One staged `/api/parameters` (or MQTT) write per tick.
    ///
    /// One, not the mailbox's four: each is an NVS commit, and four of them would sit
    /// before the heater decision. Applied on a copy and refused whole if the result
    /// is unsafe; every outcome is acknowledged, or the handler times out.
    fn apply_parameter_write(&mut self, effects: &mut Effects) {
        let Some(pairs) = self.parameters.take_one() else {
            return;
        };
        let before = (self.config.pid.enabled, self.config.brew.setpoint);
        let mut candidate = self.config.clone();
        let applied = cc_config::assign::apply(&mut candidate, &pairs);
        for (key, err) in &applied.failed {
            // The handler logged each rejection; this fires only if the verdicts differ.
            warn!("config: {key} was not written: {err}");
        }
        if applied.updated > 0 {
            if let Err(violation) =
                cc_safety::validate_config(&cc_machine::safety_config(&candidate))
            {
                let keys = violation.implicated_keys();
                error!(
                    "config: REFUSED — the resulting configuration would be unsafe \
                     ({violation:?}); implicated key(s): {keys:?}. Nothing was \
                     written and nothing was changed."
                );
                self.parameters.note_refused(keys);
            } else {
                info!(
                    "config: {} parameter(s) written: {applied:?}",
                    applied.updated
                );
                self.config = candidate;
                persist_config(&mut self.store, &self.config);
                push_into_machine(&mut self.control, &self.config, before, &pairs, effects);
                self.feed(cc_machine::Command::NormalOperation, effects);
                // Now, not on the next heartbeat: a UI that refetches on save would
                // otherwise read the old value back and the toggle would spring back.
                self.parameters
                    .publish_live(cc_web::parameters_json(&self.config));
            }
        }
        self.parameters.note_applied();
    }

    /// Step 3: store a credential typed on the console, and reboot onto it.
    ///
    /// Here because the store is: the UART task cannot write what it does not own.
    /// A failed write leaves the machine on what it has, which is the safe direction.
    fn take_console_credential(&mut self) {
        let Some(staged) = self.handoff.take() else {
            return;
        };
        match network::apply_staged(&mut self.store, staged) {
            Ok(()) => {
                info!("config: a wifi credential from the console was stored; rebooting");
                self.shutdown_and_restart(REBOOT_DISPLAY_MS);
            }
            Err(err) => error!("config: the credential from the console was NOT stored: {err}"),
        }
    }

    /// Step 4: every reading the reducer decides on, on their own cadences.
    fn sense(&mut self, now: Millis) -> Reading {
        // During an admitted OTA the httpd task is writing flash; keep the last reading.
        if !self.ota_admitted {
            self.temp.poll(now, &mut self.sensor_fault_logged);
        }
        let pressure_bar = self
            .pressure
            .as_mut()
            .and_then(|sensor| match sensor.poll(now) {
                Ok(cc_protocol::abp2::Poll::Sample(sample)) => {
                    Some(f64::from(sample.pressure.raw()))
                }
                Ok(_) => None,
                Err(err) => {
                    debug!("control: ABP2 read: {err:?}");
                    None
                }
            });
        let tank_full = self.switches.water_tank_full();
        let edges = self.switches.poll(now);
        Reading {
            temperature: self.temp.last_reading(),
            pressure_bar,
            tank_full,
            edges,
            weight_g: scale_weight(self.sampler.as_ref()),
        }
    }

    /// The sample the reducer sees.
    fn sensors(&self, reading: &Reading) -> cc_machine::Sensors {
        cc_machine::Sensors {
            // 🔴 `0.0` until the first conversion, paired with
            // `has_temperature_error` for exactly that window: the error sends the
            // reducer to `SENSOR_ERROR` and zeroes the duty. 0 °C is a plausible
            // temperature, so without the pairing the PID would ask for full power
            // and the emergency threshold would not catch it.
            temperature: reading
                .temperature
                .map_or(Celsius::new(0.0), |(celsius, _)| {
                    #[allow(
                        clippy::cast_possible_truncation,
                        reason = "a DS18B20 reading is a multiple of its own \
                              0.0625 C resolution, so f32 holds it exactly"
                    )]
                    Celsius::new(celsius as f32)
                }),
            water_tank_full: reading.tank_full,
            // 🔴 The driver's latched flag is the load-bearing term. `last_reading` is
            // a cache that `poll` never invalidates, so a probe that went silent at
            // 95 °C reads healthy forever: no `SENSOR_ERROR`, a frozen `sample_seq`
            // that stalls S1's debounce, and a PID heating against a frozen number.
            // The plausibility term covers the boot window, before ten failures latch.
            has_temperature_error: self.temp.has_error()
                || reading.temperature.is_none_or(|(_, plausible)| !plausible),
            has_scale_error: self
                .sampler
                .as_ref()
                .is_some_and(|s| s.telemetry().faulted()),
            // No scale reports no weight. `brew.by_weight` without a scale is refused
            // by `validate_config`, not left to fail here.
            #[allow(
                clippy::cast_possible_truncation,
                reason = "`Sensors::brew_weight` is an `f32`; an HX711 at Gain128 \
                          resolves 0.19 g, so f32 is exact to well below the cell's \
                          own resolution"
            )]
            brew_weight: reading.weight_g.unwrap_or(0.0) as f32,
            // The probe's conversion counter: S1 debounces readings, not ticks.
            sample_seq: self.temp.sample_seq(),
        }
    }

    /// Step 6b: admit an OTA session against the state this tick left, in its own
    /// applier pass.
    ///
    /// Folding `SafeHardwareShutdown` into the tick's list would let a later effect
    /// in the same pass (entering `BREW_PREINFUSION`) re-open the pump and valve while
    /// 1.8 MB of flash is erased. A refusal is the answer the httpd task turns into a
    /// `409`. While a session is admitted, every tick re-applies the shutdown.
    fn admit_or_hold_ota(&mut self, requested: bool) {
        if requested {
            match cc_machine::ota::begin_session(self.control.state()) {
                Ok(session_effects) => {
                    info!("control: OTA admitted — safe hardware shutdown");
                    cc_machine::apply(
                        &mut self.actuators,
                        &mut self.side,
                        self.control.machine(),
                        &session_effects,
                    );
                    self.net
                        .ota
                        .note_verdict(cc_hal_esp32::ota::Admission::Admitted);
                    self.ota_admitted = true;
                }
                Err(refusal) => {
                    warn!("control: OTA refused — {}", refusal.message());
                    self.net
                        .ota
                        .note_verdict(cc_hal_esp32::ota::Admission::Refused(refusal));
                }
            }
        } else if self.ota_admitted {
            self.safe_shutdown();
        }
    }

    /// Step 7b: persist the shot counter if it moved.
    ///
    /// Here, not in the applier: the store is this task's, and an NVS commit takes
    /// milliseconds that must not land between the reducer and the pins.
    fn persist_shots(&mut self) {
        let Some(shots) = self.side.take_shots_to_persist() else {
            return;
        };
        match cc_hal_esp32::nvs::save_shots_since_backflush(self.store.backend_mut(), shots) {
            Ok(()) => {
                self.side.note_shots_persisted(shots);
                info!("maintenance: shots since backflush = {shots} persisted");
            }
            Err(err) => error!(
                "maintenance: shots since backflush = {shots} was NOT persisted: {err}. \
                 The count in memory is still correct and the next counted brew will \
                 write it again, but a reboot before then loses this one."
            ),
        }
    }

    /// The status and brew LEDs, from the state this tick's transition produced.
    ///
    /// `LedOutput::steam` is discarded: GPIO1 is the console's (see
    /// `cc_hal_esp32::pins`). `isr_counter` is the tick clock in 10 ms steps modulo
    /// one second, the same counter the display blinks on, so the two cannot drift.
    fn show_leds(&mut self, reading: &Reading, began_ms: u32) {
        self.leds.apply(cc_display::leds::LedOutput::from_state(
            &cc_display::model::DisplayInput {
                state: self.control.state(),
                temperature: reading.celsius_or(0.0),
                setpoint: self.control.setpoint(),
                isr_counter: (began_ms % 1000) / 10 * 10,
                ..Default::default()
            },
            self.config.display.blinking.delta,
        ));
    }

    /// Step 8: hand a frame to the display task, after the effects are applied so
    /// the screen shows the state the machine is in, not the one it is entering.
    fn publish_frame(&mut self, reading: &Reading, began_ms: u32, backflush_due: bool) {
        if began_ms.wrapping_sub(self.cadence.frame) < FRAME_PUBLISH_MS {
            return;
        }
        self.cadence.frame = began_ms;
        let machine = *self.control.machine();
        // Refreshed every frame: fifteen display flags change at runtime.
        let display_view = display_task::display_config(&self.config);
        let (p, i, d) = self.control.pid_gains();
        let input = &mut self.display_input;
        input.temperature = reading.celsius_or(0.0);
        input.setpoint = self.control.setpoint();
        input.pid_output = f64::from(self.control.pid_output());
        input.pid_kp = p;
        input.pid_ki = i;
        input.pid_kd = d;
        input.state = self.control.state();
        // The post-brew deadline is measured against this.
        input.now_ms = began_ms;
        input.brew_time_ms = machine.brew.elapsed_ms;
        input.target_brew_time_ms = machine.brew.target_ms;
        // `BrewFinished` is excluded: the FSM is already in `PostBrew` by then.
        input.brew_active =
            machine.state.is_brew_state() && machine.state != MachineState::BrewFinished;
        input.backflush_reminder_due = backflush_due;
        let _ = cc_display::templates::step_brew_timer(input, &display_view);
        self.frame.publish(slots::FrameRequest {
            input: *input,
            blank: machine.standby.should_turn_off_display(),
            config: display_view,
        });
    }

    /// The three reboot paths, honoured between ticks and never inside a handler or
    /// the applier, each after a safe shutdown.
    fn take_reboot_requests(&mut self) {
        // A successful OTA asks for its own restart once the `200` has gone out.
        if self.net.ota.take_restart() {
            info!("control: OTA completed — restarting into the new image");
            self.shutdown_and_restart(OTA_RESTART_DELAY_MS);
        }
        if self.net.shared.take_reboot_request() {
            info!("control: reboot requested — restarting");
            self.shutdown_and_restart(HTTP_RESTART_DELAY_MS);
        }
        // The power switch's long press. Recorded by the applier rather than acted
        // on there, so the effects after it (a `CloseWaterValve`) still run.
        if self.side.take_reboot_request() {
            info!("control: the state machine asked for a reboot (power switch long press)");
            self.shutdown_and_restart(REBOOT_DISPLAY_MS);
        }
    }

    /// `SafeHardwareShutdown` through the real applier.
    fn safe_shutdown(&mut self) {
        let machine = *self.control.machine();
        cc_machine::apply_one(
            &mut self.actuators,
            &mut self.side,
            &machine,
            Effect::SafeHardwareShutdown,
        );
    }

    /// Shut down, pause and reset.
    ///
    /// The shutdown comes first: during the pause the loop is not running — no
    /// deadman beat, no interlock — while the heater ISR keeps chopping at the last
    /// duty.
    fn shutdown_and_restart(&mut self, pause_ms: u32) -> ! {
        self.safe_shutdown();
        FreeRtos::delay_ms(pause_ms);
        cc_hal_esp32::restart::restart_now()
    }

    /// The `/api/status` slot.
    ///
    /// A whole-slot replace published every tick, so the radio's fields are carried
    /// forward: defaulting them erased `publish_radio`'s once-a-second write within
    /// 10 ms, and a bench machine associated at -45 dBm reported `wifiAssociated:
    /// false` continuously.
    fn publish_telemetry(&self, reading: &Reading, uptime: u32, backflush_due: bool) {
        let state = self.control.state();
        let machine = self.control.machine();
        let radio = self.net.shared.snapshot();
        self.net.shared.publish(Telemetry {
            machine_state: state as i32,
            temperature_c: reading.celsius_or(f64::NAN),
            setpoint_c: self.control.setpoint(),
            heater_power_pct: f64::from(self.control.pid_output()) / 10.0,
            // The operator's setting, not the derived gate: the gate is false by
            // definition in `PID_DISABLED`, and reporting it made the UI's switch
            // turn itself off. The gate is visible through `heater_power_pct`.
            pid_enabled: machine.pid.runtime_enabled,
            // The toggles' current values; the httpd task reads them from here.
            steam_mode: machine.steam_mode,
            backflush_mode: machine.backflush.on,
            brewing: state.is_brew_state() && state != MachineState::BrewFinished,
            standby: state == MachineState::Standby,
            standby_remaining_ms: machine.standby.remaining_ms,
            #[allow(
                clippy::cast_sign_loss,
                reason = "the counter starts at 0 and is only ever incremented \
                          while brewing, and reset to 0 entering \
                          BACKFLUSH_FINISHED, so it is never negative"
            )]
            shots_since_backflush: machine.shots_since_backflush.max(0) as u32,
            backflush_threshold: u32::try_from(
                self.config.maintenance.backflush_reminder.threshold,
            )
            .unwrap_or(0),
            backflush_due,
            water_tank_full: self
                .config
                .hardware
                .sensors
                .watertank
                .enabled
                .then_some(reading.tank_full),
            pressure_bar: reading.pressure_bar,
            mqtt_configured: self.mqtt_configured,
            // Live: read once at construction it was always `false`, because the
            // client connects on its own task.
            mqtt_connected: self.mqtt.as_ref().is_some_and(mqtt_link::Link::connected),
            signal: radio.signal,
            wifi_associated: radio.wifi_associated,
            wifi_offline: radio.wifi_offline,
            ip: radio.ip.clone(),
            uptime_ms: uptime,
            weight_g: reading.weight_g,
            // The weight task's field; nothing on this task writes it.
            ..Telemetry::default()
        });
    }

    /// The radio's readings and its maintenance, once a second.
    ///
    /// After the telemetry publish, which replaces the whole slot. Not every tick:
    /// four FFI round-trips and an allocation for values that change on DHCP events.
    fn service_radio(&mut self, uptime: u32) {
        if uptime.wrapping_sub(self.cadence.radio) < WIFI_POLL_MS {
            return;
        }
        self.cadence.radio = uptime;
        network::publish_radio(&self.net.shared, self.sta.as_ref());
        if let Some(radio) = self.sta.as_mut() {
            if radio.poll() {
                warn!("wifi: offline mode — unreachable off-LAN");
            }
        }
    }

    /// Step 8b: MQTT, bounded by `mqtt::TIME_BUDGET_MS` of the 10 ms period.
    ///
    /// After the telemetry publish, so `mqttConnected` and the state it describes
    /// agree, and after the actuators, because it is the only step that talks to
    /// anything off the chip.
    fn service_mqtt(&mut self, reading: &Reading, uptime: u32, backflush_due: bool) {
        let Some(link) = self.mqtt.as_mut() else {
            return;
        };
        let live = mqtt_link::Live {
            temperature_c: reading.celsius_or(f64::NAN),
            heater_power_pct: f64::from(self.control.pid_output()) / 10.0,
            standby_remaining_ms: self.control.machine().standby.remaining_ms,
            // The last decoded sample, not this tick's poll: the ABP2's 50 ms cadence
            // makes the poll `NotReady` on nineteen ticks in twenty.
            pressure_bar: self
                .pressure
                .as_ref()
                .and_then(cc_hal_esp32::Abp2Pressure::last_sample)
                .map(|sample| f64::from(sample.pressure.raw())),
            weight_g: reading.weight_g,
            brew_weight_g: 0.0,
            water_tank_full: reading.tank_full,
            backflush_reminder_due: backflush_due,
            pid_gains: self.control.pid_gains(),
        };
        let mut mqtt_effects = Effects::new();
        let report = link.service(
            &mut self.config,
            &mut self.control,
            &mut self.store,
            &mut mqtt_effects,
            &mut self.scale_modes,
            self.sampler.as_ref(),
            &live,
            uptime,
        );
        if report.cut_short || report.unresolved > 0 {
            debug!(
                "mqtt: published {}, {} unresolved topic(s), cut short: {}",
                report.published, report.unresolved, report.cut_short
            );
        }
        // A fresh list, applied in the tick that produced it, against the machine
        // *after* the inbound commands, with the facade told that state first: an
        // inbound `steamON` would otherwise be refused by the steam interlock.
        if !mqtt_effects.is_empty() {
            let after = *self.control.machine();
            self.actuators.set_state(after.state);
            cc_machine::apply(&mut self.actuators, &mut self.side, &after, &mqtt_effects);
            if self.ota_admitted {
                self.safe_shutdown();
            }
        }
    }

    /// The temperature history and the SSE broadcast, once a second. The ring keeps
    /// one sample in three, the spacing the UI's chart assumes.
    fn service_history(&mut self, reading: &Reading, uptime: u32) {
        if uptime.wrapping_sub(self.cadence.sse) < SSE_INTERVAL_MS {
            return;
        }
        self.cadence.sse = uptime;
        #[allow(
            clippy::cast_possible_truncation,
            reason = "a temperature in -273.15..=150 C is exact in f32, and the PID \
                      output is bounded to 0..=1000 by WINDOW_MS"
        )]
        let (current, target, power_pct) = (
            reading.celsius_or(f64::NAN) as f32,
            self.control.setpoint() as f32,
            f64::from(self.control.pid_output()) as f32 / 10.0,
        );
        self.net.shared.push_history(current, target, power_pct);
        network::broadcast_temps(&self.net);
    }

    /// The heartbeat line, once a second.
    ///
    /// 🔴 `duty` is the PID's output; `ticks`/`on` are the ISR's own counters, the
    /// only view of the heater pin. A non-zero duty with zero `on` is a readback
    /// failure; the reverse is impossible.
    fn log_heartbeat(&mut self, now: Millis, began_ms: u32) {
        if began_ms.wrapping_sub(self.cadence.heartbeat_log) < LOG_INTERVAL_MS {
            return;
        }
        self.cadence.heartbeat_log = began_ms;
        let heater = self.actuators.heater();
        info!(
            "control heartbeat {} — watchdog fed, state {:?}, duty {:.0} ms, \
             applied {}, gate {}, ISR ticks {}, on {} ({:.3})",
            self.tick,
            self.control.state(),
            self.control.pid_output(),
            heater.applied_duty(),
            heater.blocked_at(now).is_none(),
            heater.transport().ticks(),
            heater.transport().on_ticks(),
            heater.transport().measured_on_fraction(),
        );
    }

    /// The PID's own terms and the interlocks' refusals, once a second.
    fn log_pid(&mut self, sensors: &cc_machine::Sensors, tank_full: bool, began_ms: u32) {
        if began_ms.wrapping_sub(self.cadence.pid_log) < LOG_INTERVAL_MS {
            return;
        }
        self.cadence.pid_log = began_ms;
        let (p, i, d) = self.control.pid_terms();
        let refusals = self.actuators.refusals();
        let (pump_active, valve_active) = self.actuators.pins_read_active();
        // `setpoint - input`, the sign the P term is computed from.
        let measured_c = f64::from(sensors.temperature.raw());
        info!(
            "control: T={:.2} C  setpoint={:.2} C  error={:.2} K  \
             duty={:.1} ms ({:.1} %)  P={p:.1} I={i:.1} D={d:.1}  \
             tank_full={tank_full}  valve={:?}  \
             refused pump={} water={} steam={} heater={}  \
             pins pump={pump_active} valve={valve_active}",
            measured_c,
            self.control.setpoint(),
            self.control.setpoint() - measured_c,
            f64::from(self.control.pid_output()),
            f64::from(self.control.pid_output()) / 10.0,
            self.actuators.valve_state(),
            refusals.pump,
            refusals.water_valve,
            refusals.steam_valve,
            refusals.heater,
        );
    }

    fn log_timing(&self, (work_mean_ms, period_mean_ms): (u32, u32)) {
        let t = &self.timing;
        info!(
            "control tick: worst {} ms of the last {} \
             (baseline {} ms over the first {TICK_BASELINE_TICKS}, budget \
             {TICK_BUDGET_MS} ms, {} over budget) — mean work {work_mean_ms} ms, \
             achieved period {period_mean_ms} ms of a {CONTROL_PERIOD_MS} ms target \
             — scale: {}{}",
            t.worst_ms,
            self.tick,
            t.baseline_worst_ms,
            t.over_budget,
            self.sampler
                .as_ref()
                .map_or_else(|| "not fitted".into(), |s| s.telemetry().describe()),
            if self
                .sampler
                .as_ref()
                .is_some_and(|s| s.telemetry().faulted())
            {
                " [FAULTED]"
            } else {
                ""
            },
        );
    }
}

/// The reducer request for a steam on/off value.
fn steam_request(on: bool) -> cc_machine::Command {
    if on {
        cc_machine::Command::SteamStart
    } else {
        cc_machine::Command::SteamStop
    }
}

/// The load cell's current weight in grams, or `None` when there is none.
///
/// A 10 Hz sample is a snapshot, not a message — losing one loses nothing — so it
/// is read beside the temperature at step 4, before the reducer decides on it. The
/// *events* (a completed tare, a new calibration factor) go through
/// [`drain_scale`], because dropping one loses an operator's action.
///
/// `None` publishes as `null`, not 0 g: 0 g is a real weight.
fn scale_weight(sampler: Option<&cc_hal_esp32::Sampler>) -> Option<f64> {
    sampler.and_then(|sampler| sampler.telemetry().weight_g())
}
