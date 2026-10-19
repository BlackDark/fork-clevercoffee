//! MQTT on the control task: the client, the publish pass, and the inbound
//! commands.
//!
//! # Why this is a module and not twenty lines in the tick
//!
//! `main.rs`'s control loop is read as a list of what happens in one 10 ms
//! period, and the ordering of that list is the property that matters. What
//! MQTT does has four steps and its own vocabulary — topics, phases, budgets,
//! the four special targets — and inlining it would have put four more blocks
//! between "sense" and "decide" without making either easier to check.
//!
//! # The pass, in one place
//!
//! Connection bookkeeping, then inbound, then publish, all under one budget. What
//! there is to publish is decided by `cc_hal_esp32::mqtt::Registry`, which Home
//! Assistant discovery is generated from too, and the shape of the pass is
//! `cc_hal_esp32::mqtt::Feed` — a cursor under a time budget rather than a loop over
//! every topic on every tick.
//!
//! # What it must not do
//!
//! * **Block the control tick.** `esp_mqtt_client_publish` copies into the
//!   client's outbox and returns; the socket write happens on the `esp-mqtt`
//!   task. The one thing that *can* block is a full outbox, which is why
//!   [`cc_hal_esp32::mqtt::Feed::service`] walks a cursor under a
//!   [`cc_hal_esp32::mqtt::TIME_BUDGET_MS`] budget rather than publishing
//!   everything at once.
//! * **Allocate per publish.** The reusable buffers live in
//!   `cc_hal_esp32::mqtt::Feed`; this module hands the value producer a
//!   `&mut Payload` rather than a `String`.
//! * **Own a second copy of the machine.** Every value it publishes is read
//!   from the [`control::Control`] this task already has.

use cc_config::blob_store::BlobConfigStore;
use cc_config::Config;
use cc_hal_esp32::mqtt::{interval_for, Client, Feed, Payload, Registry, Service};
use cc_hal_esp32::nvs::EspNvsBlob;
use cc_hal_esp32::Sampler;
use cc_machine::{Command, Effects, Event, Machine};
use esp_idf_svc::sys::EspError;
use log::{error, info, warn};

use crate::control::Control;

/// The tare and calibration latches, as `TARE_ON` and `CALIBRATION_ON` set them.
///
/// Both are latches: a command sets one and it stays set. Clearing it on the
/// sampler's answer is what makes the switch usable — a mode that can never be left is
/// a switch that can never be turned off, and the sampler has an event for exactly that
/// (`SamplerEvent::Tared` / `Calibrated` / `Refused`). Recorded as §11 of
/// `docs/history/divergences.md`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScaleModes {
    /// A tare has been asked for and has not been answered.
    pub tare: bool,
    /// A calibration has been asked for and has not been answered.
    pub calibration: bool,
}

impl ScaleModes {
    /// Clear whichever latch the sampler has just answered.
    ///
    /// `what` is the name `SamplerEvent::Refused` carries, so a refusal clears the same
    /// latch a completion would — otherwise a refused tare leaves `TARE_ON` reporting
    /// `1` for ever, which is a switch that cannot be turned off.
    pub fn answered(&mut self, what: &str) {
        if what == "tare" {
            self.tare = false;
        } else if what == "calibrate" {
            self.calibration = false;
        }
    }
}

/// The scalars one tick's MQTT pass publishes.
///
/// Captured by the caller **before** the inbound commands run, so a parameter written
/// this tick cannot change the values published in the same tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct Live {
    /// The boiler temperature, °C.
    pub temperature_c: f64,
    /// The heater duty, in per cent.
    pub heater_power_pct: f64,
    /// Milliseconds left on the standby countdown.
    pub standby_remaining_ms: u32,
    /// The ABP2's last decoded sample, in bar, or absent with no sensor fitted.
    pub pressure_bar: Option<f64>,
    /// The current weight, in grams, or absent with no scale fitted.
    pub weight_g: Option<f64>,
    /// The weight gained during the current brew, in grams.
    ///
    /// Always zero: this firmware has no brew-weight tracker, so there is no pre-brew
    /// weight to subtract and the topic carries a constant. The topic stays registered
    /// because dropping it would leave a Home Assistant entity that discovery advertises
    /// and nothing ever updates. §11 of `docs/history/divergences.md`.
    pub brew_weight_g: f64,
    /// Whether the tank float reports a full tank.
    pub water_tank_full: bool,
    /// Whether the backflush reminder is due.
    pub backflush_reminder_due: bool,
    /// The **active** PID gains, `(kp, ki, kd)`, as the operator entered them.
    pub pid_gains: (f64, f64, f64),
}

/// Everything the MQTT link is, for the one task that owns it.
///
/// `None` on a machine with no broker configured. `cc_config::Mqtt::default` has
/// `enabled = false` and an empty broker, so an unprovisioned machine spends nothing
/// here — no 4 KB of task stack, no two 1 KB buffers, and one `Option` discriminant per
/// tick.
pub struct Link {
    /// The `esp-mqtt` client. See `cc_hal_esp32::mqtt::Client`'s own notes on
    /// why it is built **here** and not in `bring_up`: the control task is the
    /// only one with the store, the configuration and the machine, and the
    /// client is `Send` if it ever has to be moved.
    client: Client,
    /// The durable publish state: the cursors, the dedupe table and the reused
    /// buffers.
    feed: Feed,
    /// Whether the command subscription has been made.
    ///
    /// Made once, on the first tick after `Connected`. `esp-mqtt` restores its own
    /// subscriptions on a resumed session, so this is once per process rather than once
    /// per reconnect.
    subscribed: bool,
}

impl Link {
    /// Build the client and its publish state for `config`.
    ///
    /// # Errors
    ///
    /// `EspError` from `esp_mqtt_client_init` / `esp_mqtt_client_start`. Not fatal: the
    /// machine runs without MQTT, and `/api/status` says so.
    pub fn new(config: &Config) -> Result<Self, EspError> {
        let client = Client::new(config)?;
        let feed = Feed::new(client.topics(), config);
        info!("mqtt: {}", client.describe());
        Ok(Self {
            client,
            feed,
            subscribed: false,
        })
    }

    /// Whether there is a session, for `/api/status`'s `mqttConnected`.
    ///
    /// Read live, every tick. It used to be read once, immediately after the
    /// client was constructed — which is **structurally always `false`**, because
    /// `esp_mqtt_client_start` connects on the client's own task and no tick had
    /// run yet.
    #[must_use]
    pub fn connected(&self) -> bool {
        self.client.is_connected()
    }

    /// One budgeted step: connection bookkeeping, inbound, then publish.
    ///
    /// # Allocation
    ///
    /// **Zero per publish, and zero per inbound command that is a plain
    /// parameter.** The publish path's buffers live in [`Feed`]; the inbound
    /// [`SetCommand`]s are fixed-size and were built on the `esp-mqtt` task, not
    /// this one. The two allocations that remain are both on the inbound path:
    /// `cc_config::assign::apply` takes `&[(String, String)]`, and the discovery
    /// document list is rebuilt once per
    /// [`cc_hal_esp32::mqtt::DISCOVERY_INTERVAL_MS`].
    ///
    /// [`SetCommand`]: cc_hal_esp32::mqtt::SetCommand
    #[allow(
        clippy::too_many_arguments,
        reason = "the arguments are the control task's own borrows, and a \
                  struct of them would have to borrow `config` immutably while \
                  the inbound path below mutates it"
    )]
    pub fn service(
        &mut self,
        config: &mut Config,
        control: &mut Control,
        store: &mut BlobConfigStore<EspNvsBlob>,
        effects: &mut Effects,
        scale_modes: &mut ScaleModes,
        sampler: Option<&Sampler>,
        live: &Live,
        now: u32,
    ) -> Service {
        // Reconnect bookkeeping. `esp-mqtt` reconnects on its own task with its own
        // schedule; what is left is the retry policy and circuit breaker, driven from
        // here so a machine whose broker has gone away says so in the log rather than
        // being silently quiet.
        if self.client.due_for_reconnect() {
            warn!("mqtt: no session — esp-mqtt is backing off before trying again");
        }

        // The command subscription, once per process.
        if !self.subscribed && self.client.is_connected() {
            match self.client.subscribe() {
                Ok(()) => self.subscribed = true,
                Err(err) => warn!("mqtt: the command subscription failed: {err:?}"),
            }
        }

        // The inbound commands, before the publish: a `set` that arrives during this
        // tick is reflected in the pass that follows it.
        for command in self.client.drain_inbound() {
            let reading = command.reading.as_str();
            let value = command.value.as_str();
            let Some(key) = self.feed.registry().resolve(reading) else {
                // A topic outside the registry. Logged and ignored: discovery is what
                // says which topics exist, so an unknown one is a client's mistake.
                warn!("mqtt: topic '{reading}' is not registered — ignored");
                continue;
            };
            info!("mqtt: command {reading} = {value}");
            self.act(
                config,
                control,
                store,
                effects,
                scale_modes,
                sampler,
                key,
                value,
            );
        }

        let machine: &Machine = control.machine();
        let registry = self.feed.static_registry();
        let mut produce = |topic: &str, payload: &mut Payload| {
            write_value(
                registry,
                config,
                live,
                machine,
                *scale_modes,
                topic,
                payload,
            )
        };
        self.feed.service(
            &mut self.client,
            config,
            now,
            interval_for(machine.state),
            &mut produce,
        )
    }

    /// The four special targets first, then the configuration parameter.
    #[allow(
        clippy::too_many_arguments,
        reason = "see `service` — these are the control task's own borrows"
    )]
    fn act(
        &self,
        config: &mut Config,
        control: &mut Control,
        store: &mut BlobConfigStore<EspNvsBlob>,
        effects: &mut Effects,
        scale_modes: &mut ScaleModes,
        sampler: Option<&Sampler>,
        key: &str,
        value: &str,
    ) {
        match key {
            cc_hal_esp32::mqtt::STEAM_MODE => {
                // Arm the steam mode and ask for normal operation. A `0` only wakes the
                // machine: this target asks for steam rather than setting it to a value,
                // so there is no steam-*off* here.
                if truthy(value) {
                    control.feed(config, Event::Command(Command::SteamStart), effects);
                }
                wake(control, config, effects);
                info!("mqtt: STEAM_MODE -> {}", truthy(value));
            }
            cc_hal_esp32::mqtt::BACKFLUSH_ON => {
                // A `backflush.cycles` of zero cannot start a cycle, so the command is
                // ignored and the next pass republishes the value the machine actually
                // holds.
                if config.backflush.cycles <= 0 {
                    warn!("mqtt: BACKFLUSH_ON rejected — backflush.cycles must be > 0");
                } else {
                    let on = truthy(value);
                    // Enable is `BackflushEnter`, disable is `BackflushStop`: the same
                    // mapping `POST /api/backflush` uses.
                    let request = if on {
                        Command::BackflushEnter
                    } else {
                        Command::BackflushStop
                    };
                    control.feed(config, Event::Command(request), effects);
                    wake(control, config, effects);
                    info!("mqtt: BACKFLUSH_ON -> {on}");
                }
            }
            cc_hal_esp32::mqtt::TARE_ON => {
                scale_modes.tare = truthy(value);
                if scale_modes.tare {
                    match sampler.map(Sampler::request_tare) {
                        Some(true) => info!("mqtt: TARE_ON — tare requested"),
                        Some(false) => warn!("mqtt: the tare request was dropped"),
                        None => warn!("mqtt: TARE_ON with no scale fitted"),
                    }
                }
                wake(control, config, effects);
            }
            cc_hal_esp32::mqtt::CALIBRATION_ON => {
                // The scale has no known weight of its own: the one in
                // `hardware.sensors.scale.known_weight` is what the web route uses too,
                // and it is this task's to read.
                scale_modes.calibration = truthy(value);
                if scale_modes.calibration {
                    let known = config.hardware.sensors.scale.known_weight;
                    match sampler.map(|s| s.request_calibrate(known)) {
                        Some(true) => info!("mqtt: CALIBRATION_ON — calibration requested"),
                        Some(false) => warn!("mqtt: the calibration request was dropped"),
                        None => warn!("mqtt: CALIBRATION_ON with no scale fitted"),
                    }
                }
                wake(control, config, effects);
            }
            key => {
                // The configuration-parameter case: resolve the topic to a schema key and
                // apply it. `pid.enabled` is the one key that also reaches the running
                // machine, which the push below does.
                self.apply_parameter(config, control, store, effects, key, value);
            }
        }
    }

    /// One `mqtt set <parameter> <value>`, through `cc_config::assign::apply`.
    ///
    /// Write the configuration, reset standby, ask for normal operation, and let the
    /// publish back happen on the next pass: the dedupe sees a changed value and
    /// republishes it, so no explicit echo is needed here.
    ///
    /// The write **is** persisted. The store is this task's, and an operator who sets a
    /// value from Home Assistant and watches it vanish after a reboot files a bug.
    #[allow(
        clippy::unused_self,
        reason = "it is a method of `Link` so that the four sibling arms of                   `act` and this one read as one handler; nothing here reads the                   client, and turning it into a free function would put the                   parameter list twice on the page"
    )]
    fn apply_parameter(
        &self,
        config: &mut Config,
        control: &mut Control,
        store: &mut BlobConfigStore<EspNvsBlob>,
        effects: &mut Effects,
        key: &str,
        value: &str,
    ) {
        // The one allocation pair on the inbound path: `cc_config::assign::apply`
        // takes `&[(String, String)]`. An inbound command is an operator action,
        // not a per-tick event.
        let pairs = vec![(String::from(key), String::from(value))];
        let before = (config.pid.enabled, config.brew.setpoint);
        let mut candidate = config.clone();
        let applied = cc_config::assign::apply(&mut candidate, &pairs);
        for (name, err) in &applied.failed {
            warn!("mqtt: {name} was not written: {err}");
        }
        if applied.updated == 0 {
            return;
        }
        if let Err(violation) =
            cc_safety::validate_config(&crate::control::safety_config(&candidate))
        {
            let keys = violation.implicated_keys();
            error!(
                "config: REFUSED — MQTT write of {key} would leave the configuration \
                 unsafe ({violation:?}); implicated key(s): {keys:?}. Nothing was \
                 written and nothing was changed."
            );
            return;
        }
        *config = candidate;
        if let Err(err) = store.save(config) {
            error!("config: an MQTT parameter write could NOT be persisted: {err}");
        }
        crate::config_io::push_into_machine(control, config, before, &pairs, effects);
        info!("config: {key} = {value} written from MQTT");
    }
}

/// Reset the standby countdown and ask for normal operation.
///
/// Every command the inbound path accepts ends with this pair, and it is the reason a
/// parameter written from Home Assistant also wakes a machine that had gone to sleep.
fn wake(control: &mut Control, config: &Config, effects: &mut Effects) {
    control.feed(config, Event::Command(Command::NormalOperation), effects);
}

/// Whether a command payload reads as "on".
///
/// **Any non-zero** payload is `true`, which is what the four special targets
/// document. A payload that is not a number reads as `false` rather than raising: a
/// malformed command is answered with the off state, which the next publish pass then
/// corrects.
#[must_use]
pub fn truthy(value: &str) -> bool {
    value.trim().parse::<f64>().is_ok_and(|v| v != 0.0)
}

/// Write the value for one MQTT topic, or return `false` for an unknown one.
///
/// Two groups. A **parameter** is resolved through the registry's topic-to-key map and
/// read out of the configuration with `cc_config::live_value`; a **sensor** is a live
/// reading and is named here. The four special targets are machine state rather than
/// configuration, and are read from the machine or the scale latches.
///
/// # Allocation
///
/// None. Every arm writes into the caller's [`Payload`], which is a reused
/// buffer, and the number formats fit it (`PAYLOAD_MAX`, and
/// `every_schema_parameter_value_fits_the_payload_buffer`).
#[allow(
    clippy::too_many_arguments,
    reason = "these are the values the publish pass reads (the config, the \
              machine, the live readings, the scale latches and the topic map), \
              and grouping them would have meant a struct borrowing `config` \
              while the inbound path mutates it"
)]
pub fn write_value(
    registry: &Registry,
    config: &Config,
    live: &Live,
    machine: &Machine,
    scale_modes: ScaleModes,
    topic: &str,
    payload: &mut Payload,
) -> bool {
    if let Some(key) = registry.resolve(topic) {
        return write_parameter(config, machine, scale_modes, key, payload);
    }
    write_sensor(live, machine, topic, payload)
}

/// The parameters, in the registry's order.
fn write_parameter(
    config: &Config,
    machine: &Machine,
    scale_modes: ScaleModes,
    key: &str,
    payload: &mut Payload,
) -> bool {
    match key {
        cc_hal_esp32::mqtt::STEAM_MODE => {
            payload.set_bool(machine.steam_mode);
            true
        }
        cc_hal_esp32::mqtt::BACKFLUSH_ON => {
            payload.set_bool(machine.backflush.on);
            true
        }
        cc_hal_esp32::mqtt::TARE_ON => {
            payload.set_bool(scale_modes.tare);
            true
        }
        cc_hal_esp32::mqtt::CALIBRATION_ON => {
            payload.set_bool(scale_modes.calibration);
            true
        }
        key => {
            let Some(value) = cc_config::live_value(config, key) else {
                return false;
            };
            // The `Text` arm exists so a text registration publishes its value rather
            // than nothing.
            match value {
                cc_config::json::LiveValue::Bool(v) => payload.set_bool(v),
                cc_config::json::LiveValue::Int(v) => payload.set_int(i64::from(v)),
                cc_config::json::LiveValue::Float(v) => payload.set_float(v),
                cc_config::json::LiveValue::Enum(v) => payload.set_int(i64::from(v)),
                cc_config::json::LiveValue::Text(v) => payload.set_text(v),
            }
            true
        }
    }
}

/// The polled sensors and the binary ones, by name.
///
/// Every arm is a value this task already has; none of them reads hardware, because a
/// probe read inside a 10 ms budget would be a second, unsynchronised read of a DS18B20
/// the control task is already mid-conversion on.
fn write_sensor(live: &Live, machine: &Machine, topic: &str, payload: &mut Payload) -> bool {
    match topic {
        "temperature" => payload.set_float(live.temperature_c),
        // Promille to per cent, the same conversion `/api/status` applies.
        "heaterPower" => payload.set_float(live.heater_power_pct),
        "standbyModeTimeRemaining" => {
            payload.set_float(f64::from(live.standby_remaining_ms) / 1000.0);
        }
        "shotsSinceBackflush" => {
            // The counter is `i32` and is never negative — it starts at 0, is
            // only incremented while brewing, and is reset on entering
            // `BACKFLUSH_FINISHED` — so the widening to `f64` is a formality that
            // documents it.
            payload.set_float(f64::from(machine.shots_since_backflush.max(0)));
        }
        "backflushReminderDue" => payload.set_float(f64::from(live.backflush_reminder_due)),
        "currentKp" => payload.set_float(live.pid_gains.0),
        "currentKi" => payload.set_float(live.pid_gains.1),
        "currentKd" => payload.set_float(live.pid_gains.2),
        // The state's numeric id, with `INIT` as the fallback for an unknown one. The
        // number is the wire format only because the enumeration's discriminants are;
        // that is checked in `cc-domain`, not here.
        "machineState" => payload.set_int(i64::from(machine.state as i32)),
        // The topic carries seconds; the reducer holds milliseconds.
        "currBrewTime" => payload.set_float(machine.brew.elapsed_ms / 1000.0),
        "currReadingWeight" => payload.set_float(live.weight_g.unwrap_or(0.0)),
        "currBrewWeight" => payload.set_float(live.brew_weight_g),
        "pressure" => match live.pressure_bar {
            Some(bar) => payload.set_float(bar),
            None => return false,
        },
        // A binary sensor is the literal `ON` or `OFF`, which is what
        // `cc_config::discovery`'s `payload_on` / `payload_off` declares.
        "waterTankFull" => payload.set_text(if live.water_tank_full { "ON" } else { "OFF" }),
        _ => return false,
    }
    true
}
