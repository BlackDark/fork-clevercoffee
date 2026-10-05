//! MQTT on the control task: the client, the publish pass, and the inbound
//! commands.
//!
//! Owner: **R3-13**, wired for real.
//!
//! # Why this is a module and not twenty lines in the tick
//!
//! `main.rs`'s control loop is read as a list of what happens in one 10 ms
//! period, and the ordering of that list is the property that matters. What
//! MQTT does has four steps and its own vocabulary — topics, phases, budgets,
//! the C++'s four special targets — and inlining it would have put four more
//! blocks between "sense" and "decide" without making either easier to check.
//!
//! # The C++ this is
//!
//! `LoopManager::updateNetwork` (`src/core/LoopManager.cpp:485-508`):
//!
//! ```cpp
//! mqttManager->checkConnection();
//! mqttManager->loop();
//! if (mqttManager->isEnabled() && mqttManager->isConnected()) {
//!     ... mqttManager->writeSysParamsToMQTT(true);
//! }
//! ```
//!
//! `checkConnection` is `MQTTManager.cpp:96-193`, `writeSysParamsToMQTT` is
//! `:366-570`, and the inbound handler is `MQTTManager::messageCallback` /
//! `assignParameter` (`:237-336`). What there is to publish is decided by
//! `SystemInitializer::registerMQTTParameters` / `registerMQTTSensors`
//! (`SystemInitializer.cpp:687-800`) and lives in
//! `cc_hal_esp32::mqtt::Registry`; the shape of the pass is
//! `cc_hal_esp32::mqtt::Feed`.
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

/// The C++'s `scaleTareMode_` and `scaleCalibrationMode_`.
///
/// `SensorCoordinator.h:203-233`. Both are latches: a command sets one and it
/// stays set. **In the C++ nothing ever clears them** — `git grep scaleTareMode_`
/// finds the write in `setScaleTareMode` and nothing else — so `TARE_ON` reports
/// `1` from the first command onwards, for ever. Here the latch is cleared when
/// the sampler answers, because a mode that can never be left is a switch that
/// can never be turned off, and the Rust sampler has an event for exactly that
/// (`SamplerEvent::Tared` / `Calibrated` / `Refused`).
///
/// Recorded in `intentional-diffs.md`.
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
    /// `what` is the name `SamplerEvent::Refused` carries, so a refusal clears
    /// the same latch a completion would — otherwise a refused tare would leave
    /// `TARE_ON` reporting `1` for ever, which is the C++'s bug rather than its
    /// behaviour.
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
/// Captured by the caller **before** the inbound commands run, so that a
/// parameter written this tick cannot change the values published in the same
/// tick — which is what the C++ does too, because its pass runs before its
/// callback is serviced again.
#[derive(Clone, Copy, Debug, Default)]
pub struct Live {
    /// The boiler temperature, °C.
    pub temperature_c: f64,
    /// `getPIDOutput() / 10` — `MQTTManager.cpp:747-752`.
    pub heater_power_pct: f64,
    /// `standbyCoordinator().getRemainingTimeMillis()` — `:754-756`.
    pub standby_remaining_ms: u32,
    /// `sensorCoordinator().getFilteredPressure()` — `:788-790`.
    pub pressure_bar: Option<f64>,
    /// `sensorCoordinator().getWeight()` — `:780-782`.
    pub weight_g: Option<f64>,
    /// `sensorCoordinator().getBrewWeight()` — `:783-784`.
    ///
    /// Always zero here: the C++'s is `cachedWeight_ - preBrewWeight_` while
    /// `brewWeightTrackingActive_` (`SensorCoordinator.cpp:85-92`), and this
    /// firmware has no brew-weight tracker — `Sensors::brew_weight` is written
    /// as a literal `0.0` in the tick. Declared in `intentional-diffs.md`; the
    /// alternative, not registering the topic, would leave a Home Assistant
    /// entity that is advertised and never updates.
    pub brew_weight_g: f64,
    /// `sensorCoordinator().isWaterTankFull()` — `:795-796`.
    pub water_tank_full: bool,
    /// `maintenanceCoordinator().isReminderDue()` — `:762-764`.
    pub backflush_reminder_due: bool,
    /// `systemContext_->pidKp/Ki/Kd()` — the **active** gains (`:765-773`).
    pub pid_gains: (f64, f64, f64),
}

/// Everything the MQTT link is, for the one task that owns it.
///
/// `None` on a machine with no broker configured, and that is the C++'s
/// behaviour too: `MQTTManager::setup` disables MQTT outright when
/// `mqtt.broker` is empty (`MQTTManager.cpp:79-83`) rather than building a
/// client with nowhere to connect. `cc_config::Mqtt::default` has
/// `enabled = false` and an empty broker, so an unprovisioned machine spends
/// nothing here — no 4 KB of task stack, no two 1 KB buffers, and one `Option`
/// discriminant per tick.
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
    /// Made once, on the first tick after `Connected` — `MQTTManager.cpp:162`
    /// subscribes immediately after `connect()` returns. `esp-mqtt` restores
    /// its own subscriptions on a resumed session, so this is once per process
    /// rather than once per reconnect.
    subscribed: bool,
}

impl Link {
    /// Build the client and its publish state for `config`.
    ///
    /// # Errors
    ///
    /// `EspError` from `esp_mqtt_client_init` / `esp_mqtt_client_start`. Not
    /// fatal: the machine runs without MQTT, as the C++ does when
    /// `mqttEnabled_` is false.
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
    /// this one. The two allocations that remain are both on the inbound path
    /// and both mirror the C++'s own handler: `cc_config::assign::apply` takes
    /// `&[(String, String)]`, and the discovery document list is rebuilt once
    /// per [`cc_hal_esp32::mqtt::DISCOVERY_INTERVAL_MS`].
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
        // `checkConnection`'s reconnect bookkeeping. `esp-mqtt` reconnects on its
        // own task with its own schedule; what is left is the C++'s retry policy
        // and circuit breaker (`MQTTManager.cpp:120-141`), driven from here so a
        // machine whose broker has gone away says so in the log rather than
        // being silently quiet.
        if self.client.due_for_reconnect() {
            warn!("mqtt: no session — esp-mqtt is backing off before trying again");
        }

        // `MQTTManager::loop()`'s subscribe, once per process.
        if !self.subscribed && self.client.is_connected() {
            match self.client.subscribe() {
                Ok(()) => self.subscribed = true,
                Err(err) => warn!("mqtt: the command subscription failed: {err:?}"),
            }
        }

        // The inbound commands, before the publish: a `set` that arrives during
        // this tick is reflected in the pass that follows it, which is what the
        // C++ gets for free because its callback and its publish are the same
        // loop.
        for command in self.client.drain_inbound() {
            let reading = command.reading.as_str();
            let value = command.value.as_str();
            let Some(key) = self.feed.registry().resolve(reading) else {
                // `MQTTManager.cpp:289-292`, "MQTT topic %s not found in mapping".
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

    /// `MQTTManager::assignParameter` (`MQTTManager.cpp:284-336`), in the C++'s
    /// order: the four specials first, then the configuration parameter.
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
                // `MQTTManager.cpp:296-302`: arm the steam mode and ask for
                // normal operation. There is no steam-*off* here — the C++'s
                // `setSteamFirstActivated(false)` plus
                // `setNormalOperationRequested(true)` does not leave steam mode
                // either — so a `0` only wakes the machine, which is reproduced
                // rather than tidied into a real stop.
                if truthy(value) {
                    control.feed(config, Event::Command(Command::SteamStart), effects);
                }
                wake(control, config, effects);
                info!("mqtt: STEAM_MODE -> {}", truthy(value));
            }
            cc_hal_esp32::mqtt::BACKFLUSH_ON => {
                // `MQTTManager.cpp:303-308` refuses the command when
                // `backflush.cycles` is zero and republishes the truth. Here the
                // refusal is free: the parameter is only applied when a cycle
                // count is configured, so the next pass reports the value the
                // machine actually holds.
                if config.backflush.cycles <= 0 {
                    warn!("mqtt: BACKFLUSH_ON rejected — backflush.cycles must be > 0");
                } else {
                    let on = truthy(value);
                    // `MQTTManager.cpp:306` calls `setBackflushMode(value)`,
                    // whose enable arm is `BackflushEnter` and whose disable arm is
                    // `BackflushStop`. The C++ has one entry point and two
                    // outcomes; the reducer has two requests, which is the same
                    // thing (`WebServerManager.cpp:502`'s `SetBackflush` maps the
                    // same way).
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
                // `MQTTManager.cpp:309-316`.
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
                // `MQTTManager.cpp:317-324`. `MQTTManager` has no known weight of
                // its own; the configuration's is what the web route uses
                // (`WebServerManager.cpp:565-575`) and it is this task's.
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
                // The configuration-parameter case:
                // `MQTTManager.cpp:325-336`, which is `findConfigParameter` +
                // `fromString`, and is `cc_config::assign::apply` here. The C++
                // additionally special-cases `pid.enabled` into
                // `setProcessPidEnabled`, which is what the push into the running
                // machine below does for it.
                self.apply_parameter(config, control, store, effects, key, value);
            }
        }
    }

    /// One `mqtt set <parameter> <value>`, through `cc_config::assign::apply`.
    ///
    /// `MQTTManager::assignParameter`'s tail: write the configuration, reset
    /// standby, ask for normal operation, publish the value back. The publish
    /// back is free here — the dedupe sees a changed value and republishes it on
    /// the next pass, which is what the C++'s explicit
    /// `publish(param, number2string(value), true)` is doing early.
    ///
    /// The C++ does **not** persist here: `fromString` writes the singleton's
    /// field and nothing else, so a setpoint set from Home Assistant is gone
    /// after the next reboot. That is not reproduced — the store is this task's
    /// and an operator who sets a value and watches it vanish files a bug.
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
        let applied = cc_config::assign::apply(config, &pairs);
        for (name, err) in &applied.failed {
            warn!("mqtt: {name} was not written: {err}");
        }
        if applied.updated == 0 {
            return;
        }
        if let Err(violation) = cc_safety::validate_config(&crate::control::safety_config(config)) {
            error!(
                "config: an MQTT write left the configuration UNSAFE ({violation:?}); \
                 the next boot will discard it"
            );
        }
        if let Err(err) = store.save(config) {
            error!("config: an MQTT parameter write could NOT be persisted: {err}");
        }
        crate::config_io::push_into_machine(control, config, before, &pairs, effects);
        info!("config: {key} = {value} written from MQTT");
    }
}

/// `standbyCoordinator().reset(); setNormalOperationRequested(true)`.
///
/// `MQTTManager.cpp:302`, `:308`, `:314`, `:321` and `:332` — every arm of
/// `assignParameter` ends with the pair, and it is the reason a parameter
/// written from Home Assistant also wakes a machine that had gone to sleep.
fn wake(control: &mut Control, config: &Config, effects: &mut Effects) {
    control.feed(config, Event::Command(Command::NormalOperation), effects);
}

/// The C++'s `static_cast<bool>(value)` of an `sscanf("%lf")` result.
///
/// `MQTTManager.cpp:299`, `:305`, `:311`, `:319` — every special target casts a
/// `double` to `bool`, so **any non-zero** payload is `true`. `sscanf` is used
/// because the C++'s `data_str` is bytes, and a non-numeric payload leaves
/// `data_double` at its initialised `0.0`, so a malformed command reads as
/// `false` rather than as an error.
#[must_use]
pub fn truthy(value: &str) -> bool {
    value.trim().parse::<f64>().is_ok_and(|v| v != 0.0)
}

/// Write the value for one MQTT topic, or return `false` for an unknown one.
///
/// Two groups, and the split is the C++'s: a **parameter** is resolved through
/// the registry's topic→key map and read out of the configuration with
/// `cc_config::live_value` (`MQTTManager.cpp:443-459` is `paramDef->toJson`
/// plus a type switch, and this is the same switch); a **sensor** is a live
/// reading and is named here. The four special parameters are machine state
/// rather than configuration and are read from the machine or the scale latches,
/// exactly as `MQTTManager.cpp:415-428` does.
///
/// # Allocation
///
/// None. Every arm writes into the caller's [`Payload`], which is a reused
/// buffer, and the number formats fit it (`PAYLOAD_MAX`, and
/// `every_schema_parameter_value_fits_the_payload_buffer`).
#[allow(
    clippy::too_many_arguments,
    reason = "these are the values the C++'s three registration lambdas close \
              over (the config, the machine, the live readings, the scale latches \
              and the topic map), and grouping them would have meant a struct \
              borrowing `config` while the inbound path mutates it"
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

/// The parameters: `MQTTManager.cpp:410-497`, in the registry's order.
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
            // `MQTTManager.cpp:448-457`. The `Text` arm exists so a future
            // registration publishes its text (`:456`) rather than nothing.
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
/// `MQTTManager.cpp:738-798`. Every arm is a value this task already has; none
/// of them reads hardware, because a probe read inside a 10 ms budget would be
/// a second, unsynchronised read of a DS18B20 the control task is already
/// mid-conversion on.
fn write_sensor(live: &Live, machine: &Machine, topic: &str, payload: &mut Payload) -> bool {
    match topic {
        "temperature" => payload.set_float(live.temperature_c),
        // `getPIDOutput() / 10` (`MQTTManager.cpp:748`), and
        // `WebServerManager.cpp:352`'s own conversion for `/api/status`.
        "heaterPower" => payload.set_float(live.heater_power_pct),
        "standbyModeTimeRemaining" => {
            payload.set_float(f64::from(live.standby_remaining_ms) / 1000.0);
        }
        "shotsSinceBackflush" => {
            // `static_cast<double>(getShotsSinceBackflush())` — `:758-761`. The
            // counter is `i32` (`MachineStateContext.h:788`) and is never
            // negative — it starts at 0, is only incremented while brewing, and
            // is reset on entering `BACKFLUSH_FINISHED` — so the widening is a
            // formality that documents it.
            payload.set_float(f64::from(machine.shots_since_backflush.max(0)));
        }
        "backflushReminderDue" => payload.set_float(f64::from(live.backflush_reminder_due)),
        "currentKp" => payload.set_float(live.pid_gains.0),
        "currentKi" => payload.set_float(live.pid_gains.1),
        "currentKd" => payload.set_float(live.pid_gains.2),
        // `static_cast<double>(getCurrentStateId())` — `:774-777`, with
        // `MachineStateId::INIT` as the fallback. The number is the C++'s only
        // because the two enumerations agree; that is checked in `cc-domain`,
        // not here.
        "machineState" => payload.set_int(i64::from(machine.state as i32)),
        // `getCurrBrewTime() / 1000` — `:771-774`.
        "currBrewTime" => payload.set_float(machine.brew.elapsed_ms / 1000.0),
        "currReadingWeight" => payload.set_float(live.weight_g.unwrap_or(0.0)),
        "currBrewWeight" => payload.set_float(live.brew_weight_g),
        "pressure" => match live.pressure_bar {
            Some(bar) => payload.set_float(bar),
            None => return false,
        },
        // `MQTTManager.cpp:534` — a binary sensor is the literal `ON` or `OFF`,
        // which is what `cc_config::discovery`'s `payload_on` / `payload_off`
        // declares.
        "waterTankFull" => payload.set_text(if live.water_tank_full { "ON" } else { "OFF" }),
        _ => return false,
    }
    true
}
