//! The four writers to the configuration blob, and the one reader of the scale
//! event queue.
//!
//! # What is in here and why
//!
//! * [`push_into_machine`] — `apply` writes the `Config`, but `pid.enabled` and
//!   `brew.setpoint` are *also* cached inside `cc_machine::Machine`, so a write
//!   has to be pushed across explicitly.
//! * [`persist_setpoint`], [`persist_pid_enabled`] and [`persist_config`] — the
//!   three writes to the blob store, which the control task solely owns
//!   (`BlobConfigStore::load` and `save` both take `&mut self`).
//! * [`drain_scale`] — the scale's *events*, drained every tick and persisted,
//!   because dropping a completed tare loses an operator's action.
//!
//! **This is a move, not a redesign.** The functions are byte-identical to the
//! ones that were in `main.rs`, including the two arguments that decide where
//! they run: `push_into_machine` and the three `persist_*` calls stay at their
//! call sites inside the control tick, and `drain_scale` stays at step 7b,
//! *after* the applier, because its NVS commit must not sit between the
//! reducer's decision and the pins.

use cc_machine::Event;
use log::{error, info, warn};

use crate::control;
use crate::mqtt_link;

/// Push a written configuration into the **running** machine.
///
/// # Why this is a function and not four lines in the handler
///
/// Because there are now two writers into the same task — `POST
/// /api/parameters` and an inbound `mqtt set` (`MQTTManager::assignParameter`,
/// `MQTTManager.cpp:325-336`) — and the defect it fixes was **two spellings of
/// the same step drifting apart**. The C++ has no such split because `Config` is
/// a singleton the state machine reads directly on every tick; here the reducer
/// owns its own copy, so a write has to be pushed into it explicitly.
///
/// # What it is
///
/// `apply` writes the `Config` value and the value reaches NVS, so it survives a
/// reboot — but two parameters are *also* cached in `cc_machine::Machine`, and
/// nothing else copies them across:
///
/// * `pid.enabled` is the case a human hit: `Machine::pid.mode_enabled` is the
///   flag `should_pid_be_enabled` consults, and it is only ever set by
///   `SetUserPidEnabled` — which until now only `POST /api/pid?on=…` sent. So
///   `POST /api/parameters pid.enabled=1` persisted the preference and did
///   nothing to the machine until a reboot, and the handler answered
///   `200 {"success":true}` throughout.
/// * `brew.setpoint` is cached by `Control::set_setpoint`, and `brew.setpoint` is
///   what `effective_setpoint` reads on every tick — so a write to it has to be
///   pushed too, or the display and the PID keep targeting the old temperature.
///
/// `before` is `(pid.enabled, brew.setpoint)` **read before** the apply, which is
/// the only way to tell whether either actually moved.
///
/// `written` is the `(key, value)` pairs the apply consumed, and it is here for
/// one reason: the PID's **gains** are not cached in the machine at all, so a
/// write to `pid.regular.kp` reaches the running PID only if the controller is
/// told to re-choose them. See [`touches_pid_gains`].
pub(crate) fn push_into_machine(
    control: &mut control::Control,
    config: &cc_config::Config,
    before: (bool, f64),
    written: &[(String, String)],
    effects: &mut cc_machine::Effects,
) {
    let (pid_enabled_before, brew_setpoint_before) = before;
    if config.pid.enabled != pid_enabled_before {
        control.feed(
            config,
            Event::Command(cc_machine::Command::SetUserPidEnabled(config.pid.enabled)),
            effects,
        );
        info!(
            "config: pid.enabled={} pushed into the running machine (was {pid_enabled_before})",
            config.pid.enabled
        );
    }
    if (config.brew.setpoint - brew_setpoint_before).abs() > f64::EPSILON {
        control.set_setpoint(control::effective_setpoint(
            config,
            control.machine().steam_mode,
        ));
        info!(
            "config: brew.setpoint={} pushed into the running machine (was {brew_setpoint_before})",
            config.brew.setpoint
        );
    }
    if touches_pid_gains(written) {
        // The C++ would wait for the next state change; this does not. See
        // `Control::retune_now`.
        control.retune_now();
        info!(
            "config: PID gains re-chosen for the running machine without waiting for a \
               state change"
        );
    }
}

/// Keys the violation implicates, or `None` when the configuration is safe.
///
/// Lives here because this crate maps `Config` to `SafetyConfig`. `cc-config`
/// and `cc-safety` stay peers.
fn validate_for_repair(config: &cc_config::Config) -> Option<&'static [&'static str]> {
    cc_safety::validate_config(&crate::control::safety_config(config))
        .err()
        .map(cc_safety::ConfigViolation::implicated_keys)
}

/// Finding #12. Revert implicated keys. See [`cc_config::repair_unsafe`].
pub fn repair_unsafe(config: &mut cc_config::Config) -> cc_config::config::Repair {
    cc_config::config::repair_unsafe(config, validate_for_repair)
}

/// Whether any written key is one of the PID's gains.
///
/// A `pid.` prefix rather than a list of keys, deliberately: the gains are
/// derived — `pid.regular.ki` comes from `tn` and `i_max`, the brew-detection
/// arm has its own subtree — so a hand-kept list is a list that rots. The cost
/// of being broad is one idempotent `set_tunings` call, which writes the same
/// numbers back and does not touch the integrator.
#[must_use]
pub fn touches_pid_gains(written: &[(String, String)]) -> bool {
    written.iter().any(|(key, _)| key.starts_with("pid."))
}

/// Persist `brew.setpoint` and report the outcome.
///
/// `WebServerManager.cpp:400` persists it inside the same handler that sets it,
/// so the two cannot disagree. Here the split is because the *running* setpoint
/// belongs to the control task's `Control` and the *stored* one belongs to the
/// store, and the store is the only durable thing — so the write is what makes
/// the change survive a reboot, and a failure to write is an `error!` rather than
/// a silent divergence.
///
/// **This function does not range-check.** The bound is enforced twice on the
/// way in, not here: `web::parse_setpoint` parses the field through
/// `cc_config::assign::parse` (so it is the schema's `20.0..=110.0` and cannot
/// drift from `ParamSpec`), and the caller checks the cross-parameter rule with
/// `cc_safety::validate_config` before reaching this point. A third copy of the
/// range here would be a third thing to keep in step, and this is the function
/// that made the defect durable — a value that should never have arrived was
/// written to the one slot the machine reads at every boot.
pub(crate) fn persist_setpoint(
    store: &mut cc_config::blob_store::BlobConfigStore<cc_hal_esp32::nvs::EspNvsBlob>,
    celsius: f64,
    config: &cc_config::Config,
) {
    let mut updated = config.clone();
    updated.brew.setpoint = celsius;
    match store.save(&updated) {
        Ok(()) => info!("config: brew.setpoint = {celsius} persisted"),
        Err(err) => {
            error!("config: brew.setpoint = {celsius} was applied but NOT persisted: {err}");
        }
    }
}

/// Persist `pid.enabled`, for the same reason as [`persist_setpoint`].
///
/// `setUserPidEnabled` persists the preference **and** sets the runtime flag
/// (`SystemUtils.h:34-40`), and `Command::SetUserPidEnabled` is the reducer half
/// of that. This is the other half.
pub(crate) fn persist_pid_enabled(
    store: &mut cc_config::blob_store::BlobConfigStore<cc_hal_esp32::nvs::EspNvsBlob>,
    enabled: bool,
    config: &cc_config::Config,
) {
    let mut updated = config.clone();
    updated.pid.enabled = enabled;
    match store.save(&updated) {
        Ok(()) => info!("config: pid.enabled = {enabled} persisted"),
        Err(err) => error!("config: pid.enabled = {enabled} was applied but NOT persisted: {err}"),
    }
}

/// Persist the whole configuration after `POST /api/parameters`.
///
/// The C++ writes **one NVS key per parameter**, inside the setter
/// (`Config.h:164-172`), so a request with four parameters is four `Preferences`
/// transactions and a power cut between two of them leaves a configuration where
/// two values are new and ninety-six are old — for a machine that heats to
/// 150 °C. This store holds one blob ([`cc_config::store`]), so the whole
/// request is one write, and either all of it is durable or none of it is.
///
/// A failure is an `error!` and not a `400`: the HTTP response has already gone
/// by the time this runs, and the C++ counts a NVS failure as a parameter
/// failure (`Config.h:171-172`) only because its write is synchronous with the
/// request. Reporting it here is the honest equivalent.
pub(crate) fn persist_config(
    store: &mut cc_config::blob_store::BlobConfigStore<cc_hal_esp32::nvs::EspNvsBlob>,
    config: &cc_config::Config,
) {
    match store.save(config) {
        Ok(()) => info!("config: the configuration was persisted"),
        Err(err) => error!(
            "config: the parameters were applied but NOT persisted, and a reboot will lose \
             them: {err}"
        ),
    }
}

/// Drain the sampling task's events, persisting whatever must outlive a reboot.
///
/// # Why this is a function and not an inline block in the tick
///
/// Two reasons, and the second is the one that matters. The first is length:
/// the tick is read as a list of what happens in a period, and inlining sixty
/// lines of NVS error handling into it hides that. The second is that the
/// **control task is the only holder of the configuration store** —
/// `BlobConfigStore::load` and `save` both take `&mut self`, and one owner beats a
/// lock — so this is the only place on the machine where a completed tare or a
/// new calibration factor can be written down. Making that a named function is
/// what makes it findable.
///
/// Events — a completed tare, a new factor — are the opposite of a reading:
/// dropping one loses an operator's action, so they come over a queue that is
/// drained every tick. **The weight is not one of them** and is not read here
/// any more; it is [`crate::scale_weight`], a reading, taken at step 4 with the
/// temperature and the pressure. It used to be returned from this function,
/// which meant the only consumer of the number — `Sensors::brew_weight` — could
/// not see it until after `control.tick` had already run, and the field was
/// written as a literal `0.0` instead. See [`crate::scale_weight`].
///
/// # Where this is called from, and why it did not move
///
/// It runs at step 7b, *after* the applier, and that is deliberate: the NVS
/// commit below is an erase-and-write measured in milliseconds, and it must not
/// sit between the reducer's decision and the pins that carry it out (the same
/// argument as the shot-counter write immediately above this call).
///
/// # Errors
///
/// Never. Every failure here is a persistence failure, and it is reported on the
/// console and in the log rather than propagated: a scale that cannot be tare
/// persisted is still a working scale, and stopping the control task over it
/// would turn a cosmetic failure into a machine with no temperature reading.
pub(crate) fn drain_scale(
    sampler: Option<&cc_hal_esp32::Sampler>,
    store: &mut cc_config::blob_store::BlobConfigStore<cc_hal_esp32::nvs::EspNvsBlob>,
    scale_modes: &mut mqtt_link::ScaleModes,
) {
    // A missing scale is not an error: there is nothing here to fail at, and
    // returning early is the whole of the "not fitted" case.
    let Some(sampler) = sampler else {
        return;
    };

    while let Some(event) = sampler.next_event() {
        match event {
            cc_hal_esp32::SamplerEvent::Tared { record } => {
                // The C++'s `scaleTareMode_` never clears, so `TARE_ON` reports
                // `1` from the first command for ever; the latch here is cleared
                // when the sampler answers. See `mqtt_link::ScaleModes`.
                scale_modes.answered("tare");
                // 🔴 The acceptance criterion: a tare survives a reboot. The
                // C++ holds the tare in a `long` member (`HX711_ADC.h:66`) and
                // loses it on every reset, so a power cut means re-taring by
                // hand — and `HX711Scale::init` tares at boot anyway
                // (`HX711Scale.cpp:44`), so the C++ would re-tare on every boot
                // if it ran at all.
                match cc_hal_esp32::nvs::save_tare(store.backend_mut(), record) {
                    Ok(()) => info!(
                        "scale: tare persisted to NVS ({} B) — it survives a reboot",
                        cc_protocol::sensor::hx711::TARE_RECORD_BYTES
                    ),
                    Err(err) => error!(
                        "scale: the tare was taken but could NOT be persisted: \
                         {err}. It will be lost on reboot."
                    ),
                }
            }
            cc_hal_esp32::SamplerEvent::Calibrated { factor_1, factor_2 } => {
                scale_modes.answered("calibrate");
                // The factor is a **configuration parameter**
                // (`hardware.sensors.scale.calibration` and `calibration2`),
                // not a tare, so it goes into the blob rather than beside it.
                // That is also what makes the setting survive a reboot, which
                // matters because recalibrating is a deliberate act an operator
                // performs once.
                let mut updated = match store.load() {
                    Ok(config) => config.unwrap_or_default(),
                    Err(err) => {
                        error!(
                            "scale: cannot read the configuration to store the calibration: {err}"
                        );
                        continue;
                    }
                };
                updated.hardware.sensors.scale.calibration = factor_1;
                if let Some(factor) = factor_2 {
                    updated.hardware.sensors.scale.calibration2 = factor;
                }
                match store.save(&updated) {
                    Ok(()) => {
                        info!(
                            "scale: calibration persisted — cell 1 {factor_1}{}",
                            match factor_2 {
                                Some(factor) => format!(", cell 2 {factor}"),
                                None => String::new(),
                            }
                        );
                    }
                    Err(err) => {
                        error!("scale: the calibration was applied but NOT persisted: {err}");
                    }
                }
            }
            cc_hal_esp32::SamplerEvent::Refused { what } => {
                scale_modes.answered(what);
                warn!("scale: the sampler refused a {what} request");
            }
        }
    }
}
