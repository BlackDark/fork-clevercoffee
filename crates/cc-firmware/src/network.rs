//! R3 bring-up: NVS, Wi-Fi, provisioning, MQTT, HTTP and SSE.
//!
//! Owner: **R3-08, R3-12, R3-13, R3-14** (tasks A–E).
//!
//! # Where the fail-closed rule is wired, and why here
//!
//! 04 §6 makes `cc-config` and `cc-safety` **siblings**: `cc-config` may not
//! depend on `cc-safety` (CI-enforced, 05 §6), and `cc-safety` may not depend on
//! `cc-config` — `SafetyConfig` is a five-field hand-off view
//! ([`cc_config::Config::safety_view`]), not a `Config`. So neither can host the
//! join.
//!
//! The join lives in `cc-firmware`, and that is the right place for three
//! reasons:
//!
//! 1. **It is a boot-order decision, not a storage decision.** Whether an unsafe
//!    stored configuration is *discarded and kept* (run the defaults, and on the
//!    next boot discover the same blob and discard it again) or *discarded and
//!    erased* is a question about what the firmware then does, and that is
//!    startup policy. [`bring_up_config`] makes the choice explicitly and logs
//!    which one it made.
//! 2. **`cc-firmware` is the only crate that may know the startup order.** Every
//!    other crate takes its configuration as a value.
//! 3. **It is fifteen lines and it runs once per boot, in one place**, where the
//!    log line that reports a discarded configuration belongs. The safety
//!    *decision* is not here and is fully tested in `cc-safety`; this is the
//!    three-line call of it that 08 §4.1's log messages describe.
//!
//! The decision that an unsafe blob is **kept on disk rather than erased** is
//! deliberate and worth naming: erasing it destroys the evidence, so the next
//! boot reports `Defaults` instead of `DiscardedUnsafe` and the operator never
//! learns that their configuration was rejected. The blob is inert — the
//! firmware runs the defaults regardless — and `GET /api/nvs-debug` still shows
//! it, so the diagnosis survives a reboot. Rejection is also reported at `warn`.
//!
//! # The task layout
//!
//! | Task | Owns | Cadence |
//! | --- | --- | --- |
//! | control | the heater, the watchdog, the temperature probe, and the telemetry publish | [`crate::main::CONTROL_PERIOD_MS`] (10 ms) |
//! | network | the radio, the monitor, MQTT | [`cc_hal_esp32::wifi::MONITOR_PERIOD_MS`] (1 s) |
//! | provisioning | the UART reader and the staged credential | 50 ms while armed, otherwise off |
//! | httpd | the REST API and the SSE stream | ESP-IDF's own task |
//! | esp-mqtt | the MQTT session | ESP-IDF's own task |
//!
//! The control task is the only watchdog subscriber and the only feeder, and the
//! only thing that may open the heater gate (04 §3.4). Nothing in this module
//! touches an actuator.

use std::sync::{Arc, Mutex};

use cc_config::blob_store::BlobConfigStore;
use cc_config::{Config, ConfigStore, NVS_NAMESPACE};
use cc_hal_esp32::heap::{free_heap, min_free_heap};
use cc_hal_esp32::nvs::EspNvsBlob;
use cc_hal_esp32::provisioning::{self, Action, Session};
use cc_hal_esp32::time::now_ms;
use cc_hal_esp32::web::{Command, Shared, Sse, Telemetry, Web};
use cc_hal_esp32::wifi::Sta;
use cc_safety::ConfigOrigin;
use log::{info, warn};

use esp_idf_hal::delay::FreeRtos;
use esp_idf_svc::sys::{EspError, ESP_ERR_NO_MEM};

/// What the boot decided to run, and where the configuration came from.
pub struct Booted {
    /// The configuration the machine is running.
    pub config: Config,
    /// Where it came from, and whether anything was thrown away.
    ///
    /// Named for what it is: `cc_safety::LoadedConfig::origin`, carried through
    /// unchanged so the boot log can print the violation that caused a discard.
    pub origin: ConfigOrigin,
    /// The store, and the **only** handle on it.
    ///
    /// `ConfigStore::load`/`save` take `&mut self`, so the store has one owner
    /// or it has a lock. It has an owner: the control task. The HTTP server
    /// reports on NVS through [`Booted::nvs_description`] — a string captured
    /// once the boot writes are done — and every request that would *change* the
    /// configuration (provisioning, `POST /api/factory-reset`,
    /// `POST /api/wifi-reset`) arrives as a [`Staged`] value the control task
    /// acts on. One writer, no lock, no `Arc<Mutex<_>>` on a path a web request
    /// can stall.
    pub store: BlobConfigStore<EspNvsBlob>,
    /// `store.describe()` as of the end of the boot, for `/api/nvs-debug`.
    pub nvs_description: String,
}

/// Open NVS, load the configuration, and apply the fail-closed rule.
///
/// # Errors
///
/// [`EspError`] if the NVS partition or namespace cannot be opened. That is not
/// fatal: the machine runs the compiled-in defaults, exactly as a freshly
/// erased device does, and the boot log says so.
pub fn bring_up_config() -> Result<Booted, EspError> {
    // `cc_config::StoreError` has no `EspError` equivalent and must not grow
    // one: it is a portable type with a closed set of four cases, and an ESP-IDF
    // error code is not one of them. The conversion is here, once.
    let nvs = cc_hal_esp32::nvs::open(NVS_NAMESPACE).map_err(|err| {
        warn!("config: the NVS namespace could not be opened: {err}");
        EspError::from_infallible::<ESP_ERR_NO_MEM>()
    })?;
    let mut store = BlobConfigStore::new(nvs);

    let stored = match store.load() {
        Ok(config) => config,
        Err(err) => {
            // `StoreError::Corrupt` means a blob this firmware wrote that will
            // not decode. Half a configuration is not a safer configuration
            // than none (see `cc_config::StoreError::Corrupt`), so the defaults
            // are used and the corrupt blob is *overwritten* below.
            warn!("config: stored configuration is {err} — running the defaults");
            None
        }
    };

    // The fail-closed rule. `cc_safety::load_or_default` is the whole of the
    // decision and is fully tested in `cc-safety`; this is the call.
    // **One construction, one owner.** This used to build `SafetyConfig` field by
    // field, separately from `control::safety_config`, and a reviewer's finding
    // was that the two drifted: this copy hard-coded the two relay trigger types
    // to `HighTrigger`, so `validate_config` never saw a stored low-trigger pump
    // or valve, the fail-closed rule did not discard the configuration — and
    // `main.rs` then applied that very polarity to the pins, energising the pump
    // at boot. Two constructions of a safety input is one too many.
    let safety = stored.as_ref().map(crate::control::safety_config);

    let loaded = cc_safety::load_or_default(safety.as_ref());

    let had_stored_config = stored.is_some();
    let mut config = stored.unwrap_or_default();
    match loaded.origin {
        ConfigOrigin::Stored => {}
        ConfigOrigin::Defaults => {
            // First boot, or nothing stored. Write the defaults so the next boot
            // is a `Stored` and the "written" line appears exactly once.
            if !had_stored_config {
                info!("config: nothing stored — writing the compiled-in defaults");
                if let Err(err) = store.save(&config) {
                    warn!("config: could not write the defaults: {err}");
                }
            }
        }
        ConfigOrigin::DiscardedUnsafe(violation) => {
            // 🔴 Refuse to run it, and **keep it on disk** — see the module
            // documentation for why erasing it would destroy the evidence.
            // `ConfigViolation` has no `Display`. It is `Debug`, and it is the
            // only one of its four variants that can be produced from a `Config`
            // the firmware itself wrote, so the name is enough to act on; the
            // full variant detail is in `/api/nvs-debug` once that reports it.
            //
            // **But the network credential survives.** This was found the hard
            // way: refusing a stored configuration discarded the whole blob, the
            // machine came up on the compiled-in defaults, and the defaults carry
            // no SSID — so the radio never associated and the **only** way to fix
            // the parameter that caused the refusal was a serial console. The
            // refusal is about a relay's polarity, not about connectivity, and
            // discarding the connectivity along with it turns a configuration
            // mistake into an unreachable machine.
            //
            // So the credential is carried over, and **not** written back: the
            // blob on disk stays exactly as it was, so `/api/nvs-debug` and the
            // next boot still see the configuration that was refused. The
            // operator reaches the UI, fixes the parameter, and the blob becomes
            // valid again.
            let credential = config.system.wifi.clone();
            warn!(
                "config: (configuration is unsafe to run: {violation:?}) -> discarding \
                 all of it and running defaults. The blob is left on disk so \
                 /api/nvs-debug and the next boot can still see it."
            );
            let mut defaults = Config::default();
            let ssid = credential.ssid.trim();
            if !ssid.is_empty() {
                info!(
                    "config: keeping the stored Wi-Fi credential ({ssid}) so the \
                     machine stays reachable and the unsafe setting can be fixed \
                     over HTTP"
                );
                defaults.system.wifi = credential;
            }
            config = defaults;
        }
    }

    // Described *after* every write above, so `/api/nvs-debug` cannot report a
    // blob that is not there yet. Read once: `ConfigStore::describe` reads the
    // blob to size it, and the answer cannot change until the next write.
    let nvs_description = store_location(&store);
    info!("config: {} ({nvs_description})", origin_text(loaded.origin));
    Ok(Booted {
        config,
        origin: loaded.origin,
        store,
        nvs_description,
    })
}

/// A one-word description of a [`ConfigOrigin`] for the boot log.
fn origin_text(origin: ConfigOrigin) -> &'static str {
    match origin {
        ConfigOrigin::Defaults => "defaults",
        ConfigOrigin::Stored => "stored",
        ConfigOrigin::DiscardedUnsafe(_) => "stored but unsafe — DISCARDED",
    }
}

/// The store's own description, for the boot log and `/api/nvs-debug`.
pub fn store_location(store: &BlobConfigStore<EspNvsBlob>) -> String {
    store
        .describe()
        .unwrap_or_else(|_| String::from("unavailable"))
}

/// The shared HTTP state and the command channel the control task drains.
pub struct Network {
    /// The state every handler reads.
    pub shared: Arc<Shared>,
    /// The SSE counters.
    pub sse: Arc<Sse>,
}

impl Network {
    /// A fresh, unstarted network surface.
    #[must_use]
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared::new()),
            sse: Arc::new(Sse::new(cc_hal_esp32::web::SseMode::Chunked)),
        }
    }
}

impl Default for Network {
    fn default() -> Self {
        Self::new()
    }
}

/// Start the HTTP server and return the handle.
///
/// # Where this is called from, and why it matters
///
/// `EspHttpServer` holds a `*mut c_void` (`esp-idf-svc` `src/http/server.rs:338`)
/// and is therefore **neither `Send` nor `Sync`**, so it cannot be moved to
/// another thread. It is also not `Sync`, so it cannot go in a `static`.
///
/// That leaves exactly one place it can live: the thread that constructed it,
/// held in a local. Which is fine and needs no `unsafe`, because `main` blocks
/// on `control.join()` for the life of the process — the handle's lifetime is
/// the process's lifetime by construction, and `EspHttpServer::drop`
/// (`server.rs:732-736`) never runs. The alternative, a `Box::leak`, would
/// achieve the same thing with a comment explaining why it is safe; a local is
/// the same guarantee with the compiler checking it.
///
/// The handlers themselves do run on ESP-IDF's httpd task and are `Send +
/// 'static` closures over `Arc`s — that is `server.rs:530-538`'s requirement,
/// and it is what the `Arc<dyn Fn(Command) + Send + Sync>` sink is for.
///
/// # Errors
///
/// [`EspError`] from `EspHttpServer::new` or any `httpd_register_uri_handler`.
/// `ESP_ERR_HTTPD_HANDLERS_FULL` is the one worth naming: it fires at boot if
/// the route table outgrows its budget, and
/// `cc_hal_esp32::web::the_route_table_fits_the_servers_handler_budget` is the
/// host test that catches it before a flash.
pub fn start_http(
    network: &Network,
    config: &Config,
    nvs_description: &str,
    commands: Arc<cc_hal_esp32::task::CommandQueue>,
    parameters: &Arc<cc_hal_esp32::task::ParameterHandoff>,
) -> Result<Web, EspError> {
    let shared = Arc::clone(&network.shared);
    let sse = Arc::clone(&network.sse);
    let config = Arc::new(config.clone());
    // Non-blocking, drop-newest. A full queue means the control task is behind,
    // which is a condition to shed, not to report (04 §3.2).
    let sink: Arc<dyn Fn(Command) + Send + Sync + 'static> = Arc::new(move |command: Command| {
        let _ = commands.try_send(command);
    });
    let web = Web::start(shared, sse, &config, nvs_description, &sink, parameters)?;
    info!(
        "http: {} routes registered; the large-response floor is {} B",
        cc_hal_esp32::web::routes().len(),
        cc_hal_esp32::web::HEAP_FLOOR_BYTES
    );
    Ok(web)
}

/// The reading the control task has, and the configuration facts that go with it.
///
/// Grouped rather than passed as eight scalars because the eight scalars are
/// the *shape of one telemetry record*, and a function whose signature is
/// `(i32, f64, f64, f64, u32, Option<&Sta>, bool, bool)` cannot be read without
/// counting the arguments against the field order of
/// [`cc_hal_esp32::web::Telemetry`].
#[allow(
    clippy::struct_excessive_bools,
    reason = "`Reading` is a *report of facts about the machine*, one field per \
              key the C++'s `/api/status` publishes -- and seven of those are \
              booleans because seven of the C++'s are \
              (`WebServerManager.cpp:350-361`). Turning them into enums would \
              make the publisher unreadable and would not make the data any \
              more correct. The same reasoning is on `cc_hal_esp32::Telemetry`, \
              whose payload this fills, and it is pinned there by \
              `the_cpp_keys_are_the_schema`."
)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Reading {
    /// `MachineState`'s integer discriminant, as `/api/status` publishes it.
    pub state: i32,
    /// The boiler temperature in °C, or `f64::NAN` before the first conversion.
    pub temperature_c: f64,
    /// The active setpoint in °C — `brew.setpoint + brew.temp_offset`, or
    /// `steam.setpoint` while steam mode is on
    /// (`ProcessController::updateSetpoint`, `ProcessController.cpp:235-244`).
    pub setpoint_c: f64,
    /// The PID output in per cent: `machine.pid.output / 10`, the C++'s own
    /// conversion (`WebServerManager.cpp:352`).
    ///
    /// **What this is not:** a measurement of the pin. It is what the controller
    /// computed, and `cc_hal_esp32::Actuators` is what decides whether that
    /// reaches the heater — so with a tank interlock, an emergency latch or a
    /// `test_only` inhibit in force, this can be non-zero while the boiler is
    /// cold. That is the honest reading and it is the C++'s: the C++ publishes
    /// `processPidOutput`, not a pin.
    pub heater_power_pct: f64,
    /// `context.isPidRuntimeEnabled()`.
    pub pid_enabled: bool,
    /// `MachineStateContext::steamON_` (`MachineStateContext.h:785`).
    ///
    /// Published because `POST /api/steam` with no field is a **toggle** in the
    /// C++ (`!isSteamModeActive()`, `WebServerManager.cpp:444`) and the httpd
    /// task needs the current value to compute the target.
    pub steam_mode: bool,
    /// `systemContext_->backflushMode()` — whether backflush *mode* is armed.
    ///
    /// Published for `POST /api/backflush`'s toggle
    /// (`WebServerManager.cpp:490`), on the same reasoning as [`Self::steam_mode`].
    pub backflush_mode: bool,
    /// `isBrewState(state) && state != BREW_FINISHED`
    /// (`BrewHandler::isBrewActive`).
    pub brewing: bool,
    /// `state == STANDBY`.
    pub standby: bool,
    /// `StandbyCoordinator::standbyModeRemainingMillis()`.
    pub standby_remaining_ms: u32,
    /// `MaintenanceCoordinator::shotsSinceBackflush()`.
    pub shots_since_backflush: u32,
    /// `maintenance.backflush_reminder.threshold`.
    ///
    /// `WebServerManager.cpp:362`. It was never published, so `/api/status`
    /// answered `backflushReminderThreshold: 0` — a threshold of zero makes the
    /// reminder look due forever *and* reads as "the machine does not know its
    /// own limit", which is what the human reported as a 0/0 pair.
    pub backflush_threshold: u32,
    /// `MaintenanceCoordinator::isReminderDue()` —
    /// `isReminderDueForCount(shots, enabled, threshold)`
    /// (`MaintenanceCoordinator.cpp:68-73`).
    pub backflush_due: bool,
    /// `SensorCoordinator::isWaterTankFull()`, or `None` when no float is fitted.
    pub water_tank_full: Option<bool>,
    /// The ABP2's reading in bar, or `None` when no pressure sensor is fitted or
    /// it has not answered.
    pub pressure_bar: Option<f64>,
    /// `mqtt.enabled` and a non-empty `mqtt.broker`.
    pub mqtt_configured: bool,
    /// Whether MQTT has a session.
    pub mqtt_connected: bool,
}

/// The telemetry the control task publishes, from one [`Reading`].
///
/// The radio's four fields are **not** set here. See [`publish_radio`]: they
/// belong to whoever holds the radio, and the control task does not.
#[must_use]
pub fn telemetry_from(reading: Reading, uptime_ms: u32, weight_g: Option<f64>) -> Telemetry {
    Telemetry {
        machine_state: reading.state,
        temperature_c: reading.temperature_c,
        setpoint_c: reading.setpoint_c,
        heater_power_pct: reading.heater_power_pct,
        pid_enabled: reading.pid_enabled,
        steam_mode: reading.steam_mode,
        backflush_mode: reading.backflush_mode,
        brewing: reading.brewing,
        standby: reading.standby,
        standby_remaining_ms: reading.standby_remaining_ms,
        shots_since_backflush: reading.shots_since_backflush,
        backflush_threshold: reading.backflush_threshold,
        backflush_due: reading.backflush_due,
        // `None` publishes as `"waterTankFull":null`, which is what the C++'s
        // "no float switch fitted" is: the key is only emitted when
        // `hardwareSensorsWatertankEnabled` (`WebServerManager.cpp:356-372`).
        water_tank_full: reading.water_tank_full,
        pressure_bar: reading.pressure_bar,
        uptime_ms,
        mqtt_configured: reading.mqtt_configured,
        mqtt_connected: reading.mqtt_connected,
        // `None` publishes as `"weight":null`, which is what the C++'s
        // "no reading" is (`WebServerManager.cpp:356-372` omits the key when no
        // scale is enabled) and what `an_absent_reading_is_null_and_never_a_
        // fabricated_zero` pins.
        weight_g,
        ..Telemetry::default()
    }
}

/// Publish the radio's readings into the shared snapshot.
///
/// The radio is `Sta`, which is a live handle to the netif and the driver. The
/// control task publishes telemetry from a [`Reading`] it builds itself and has
/// no handle on `Sta`; reaching across for one is the coupling 04 §3.2 forbids.
/// So the holder of the radio publishes these four fields, and the control task
/// leaves them alone — which it does by *not writing them*, because
/// [`Shared::publish`] is a whole-slot replace, so the two publishers have to
/// agree on who owns which fields. That agreement is the four fields on
/// [`Telemetry`] that are not in [`Reading`].
///
/// **This is what `/api/status`'s `wifiAssociated`, `wifiSignal`, `wifiOffline`
/// and `ip` come from**, and until it was called the control task's
/// `telemetry_from(.., None)` left them at their defaults — so a machine
/// associated at −53 dBm reported `wifiAssociated: false, wifiSignal: 0,
/// ip: null`. Measured on hardware, not inferred.
///
/// Note the ownership consequence, because it is the part that bites later: the
/// snapshot is a single slot and both publishers write it, so the two must not
/// race. They do not today — the control task writes once per
/// [`crate::main::CONTROL_PERIOD_MS`] and the radio once per second, and each write is a single
/// `Mutex` critical section over the whole slot, so the worst case is one
/// publisher's fields being one tick stale, never torn.
pub fn publish_radio(shared: &Shared, sta: Option<&Sta>) {
    let Ok(mut slot) = shared.telemetry.lock() else {
        return;
    };
    if let Some(sta) = sta {
        slot.signal = sta.signal().as_bars();
        slot.wifi_associated = sta.is_associated();
        slot.wifi_offline = sta.is_offline();
        slot.ip = sta.ip().map(|ip| format!("{ip}"));
    } else {
        // No radio at all. Reporting "not associated, no address" is the truth,
        // and is what a machine that never provisioned a network should say.
        slot.signal = 0;
        slot.wifi_associated = false;
        slot.wifi_offline = false;
        slot.ip = None;
    }
}

/// Publish the SSE `new_temps` event, if an HTTP server is running.
///
/// Called from the control task at the C++'s cadence
/// (`WebServerManager.cpp:1128-1143`, driven from `LoopManager::updateWebsite`).
/// It reads the snapshot the control task has *just* published, so the frame
/// cannot disagree with `/api/status`.
/// Publish the SSE `new_temps` event, if an HTTP server is running.
///
/// Called from the control task at the C++'s cadence
/// (`WebServerManager.cpp:1128-1143`, driven from `LoopManager::updateWebsite`).
/// It reads the snapshot the control task has *just* published, so the frame
/// cannot disagree with `/api/status`.
///
/// This is the C++'s division of labour exactly: the **producer** is the main
/// loop and the **transport** is the event source. Here the producer is this
/// function and the transport is the broadcaster task `Web::start` spawned, so
/// the cadence lives where the C++ puts it rather than inside the stream.
pub fn broadcast_temps(network: &Network) {
    let snapshot = network.shared.snapshot();
    network.sse.broadcast(Sse::frame(
        "new_temps",
        &cc_hal_esp32::web::temperatures_json(&snapshot),
    ));
    network.sse.note_push_attempt();
}

/// Log the heap the way ADR-0002 wants it reported, once a minute.
pub fn log_heap_once_a_minute(network: &Network) {
    info!(
        "heap: {} B free, {} B minimum — the large-response floor is {} B \
         ({} served, {} refused)",
        free_heap(),
        min_free_heap(),
        cc_hal_esp32::web::HEAP_FLOOR_BYTES,
        network.shared.large_responses(),
        network.shared.large_refused(),
    );
}

/// What the provisioning task captured, waiting for the control task to store it.
///
/// A credential is **not** a [`cc_hal_esp32::web::Command`]. Those are `Copy` and
/// eight bytes because they cross a `hal::task::queue::Queue` (04 §3.2), and a
/// `String` cannot. That constraint is why a credential takes this route
/// instead, and it is a good one: a credential that could travel as a command
/// would be a credential an HTTP handler could set.
pub enum Staged {
    /// `wifi set <ssid>`, the password line, then `wifi apply`.
    Set(cc_hal_esp32::provisioning::Pending),
    /// `wifi clear`, then `wifi apply`.
    Clear,
}

impl core::fmt::Debug for Staged {
    /// Redacted, and never a `Pending`'s own `Debug` by accident.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Set(_) => f.write_str("Staged::Set { ssid: [redacted], password: [redacted] }"),
            Self::Clear => f.write_str("Staged::Clear"),
        }
    }
}

/// The one-slot channel from the provisioning task to the control task.
///
/// The store has exactly one owner — the control task — and `ConfigStore::load`
/// and `save` both take `&mut self`. So a credential typed on UART0 cannot be
/// written where it is typed: it is staged here, and the control task picks it
/// up at the top of its next tick with the store already in its hand.
///
/// **One slot, and a refusal rather than an overwrite.** [`Handoff::stage`]
/// returns `false` when something is already staged and the operator is told
/// so. That is the opposite policy to [`cc_hal_esp32::task::CommandQueue`]'s
/// drop-newest, and deliberately: losing a *set* because a *clear* arrived
/// behind it, or the reverse, would leave the machine holding a credential
/// nobody can account for.
///
/// A `std::sync::Mutex` and not a `FreeRTOS` one because the critical section is
/// two instructions, and holding a `FreeRTOS` mutex across two instructions is a
/// priority inversion bought for nothing. It is never held across anything that
/// can block, and both methods return rather than wait if the lock is poisoned —
/// a poisoned handoff means the task that touched it panicked, and a second
/// credential written over a machine in that state would be worse than none.
#[derive(Clone, Default)]
pub struct Handoff(Arc<Mutex<Option<Staged>>>);

impl Handoff {
    /// A handoff with nothing staged.
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(None)))
    }

    /// Stage a value for the control task.
    ///
    /// Returns `false` if there is already something staged, in which case
    /// nothing is overwritten.
    #[must_use]
    pub fn stage(&self, staged: Staged) -> bool {
        let Ok(mut slot) = self.0.lock() else {
            return false;
        };
        if slot.is_some() {
            return false;
        }
        *slot = Some(staged);
        true
    }

    /// Take the staged value, if any. Called by the control task.
    #[must_use]
    pub fn take(&self) -> Option<Staged> {
        self.0.lock().ok()?.take()
    }
}

impl core::fmt::Debug for Handoff {
    /// Whether something is staged, never what.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let staged = self.0.lock().is_ok_and(|slot| slot.is_some());
        write!(f, "Handoff({})", if staged { "staged" } else { "empty" })
    }
}

/// Write a staged credential, and report whether it took.
///
/// Runs on the control task, which owns the store, and is the **only** place a
/// credential reaches NVS. The load/modify/save is one blob write, so it is
/// atomic from a reader's point of view and a failed save leaves the previous
/// configuration in place — the property
/// `cc_config::blob_store`'s own test pins.
///
/// A `Corrupt` load is *not* treated specially: the stored blob will not decode,
/// so the defaults are written with the new credential on top, which is the same
/// thing the boot path already does.
pub fn apply_staged(
    store: &mut BlobConfigStore<EspNvsBlob>,
    staged: Staged,
) -> Result<(), cc_config::StoreError> {
    let mut config = store.load()?.unwrap_or_default();
    match staged {
        Staged::Set(mut pending) => config.set_wifi_credential(
            // Moved out, never `expose`d: there is no point at which a second
            // copy of the credential exists outside the `Config` it is about to
            // be written into, and no `String` is left behind to be leaked.
            std::mem::take(pending.ssid.expose_mut()),
            std::mem::take(pending.password.expose_mut()),
        ),
        Staged::Clear => config.clear_wifi_credential(),
    }
    store.save(&config)
}

/// Run the UART provisioning loop until a credential is staged.
///
/// The task ends by *returning* once a command has been handed to the control
/// task: 04 §3.2 says the provisioning task "exits after success", and it is the
/// control task that stores the credential and resets the machine, because the
/// control task owns the store. The C++ reboots from its own task
/// (`CleverCoffeeWiFiManager.cpp:148-151`, `ESP.restart()` after a portal save)
/// because in the C++ that task *is* the one holding the configuration; doing
/// the same here would reset the machine before the write, every time.
#[allow(
    clippy::needless_pass_by_value,
    reason = "the `Serial` IS the task: it owns UART0 for the task's whole life \
              and is never handed anywhere else. A `&mut` would suggest it \
              could be shared, which is the one thing a UART must not be."
)]
pub fn run_provisioning(serial: cc_hal_esp32::provisioning::Serial<'_>, handoff: Handoff) {
    let mut session = Session::new();
    // `wifi clear` is a two-step command — the session drops the staged
    // credential and answers "apply to persist" — and which of the two things
    // `wifi apply` should write is the session's decision, not this task's:
    // `Session::take_action` answers `Action::Set` or `Action::Clear`.
    provisioning::announce();
    serial.write_reply(&provisioning::line(
        "ok type `wifi status` for the current state",
    ));
    info!("serial: waiting for a `wifi set <ssid>` command");

    let mut buf = [0u8; 64];
    let mut last_heartbeat_ms = now_ms();
    loop {
        let read = serial.read_available(&mut buf);
        if read > 0 {
            // `Session::push` already returns fully-formed reply lines —
            // `cc_hal_esp32::provisioning::reply_text` puts the `CCWIFI `
            // prefix on — so they are written as they are. Wrapping them in
            // `provisioning::line` again is what put two prefixes on every
            // session reply.
            for reply in session.push(&buf[..read]) {
                serial.write_reply(&reply);
            }
            let action = session.take_action();
            if action != Action::None {
                let staged = match action {
                    Action::Set => session.take_pending().map_or(Staged::Clear, Staged::Set),
                    Action::Clear | Action::None => Staged::Clear,
                };
                stage_and_stop(
                    &serial,
                    &handoff,
                    staged,
                    match action {
                        Action::Clear => "ok cleared — the machine forgets it, then reboots",
                        _ => "ok accepted — the machine stores it, then reboots",
                    },
                );
                // 04 §3.2: "the provisioning task ... exits after success". After
                // a refusal the machine is in the same unprovisioned state it was
                // in, so the operator re-boots when they are ready.
                return;
            }
            session.expire_window();
        }
        if now_ms().wrapping_sub(last_heartbeat_ms) >= PROVISION_HEARTBEAT_MS {
            last_heartbeat_ms = now_ms();
            serial.write_reply(&provisioning::line("ok still waiting"));
        }
        FreeRtos::delay_ms(PROVISION_POLL_MS);
    }
}

/// Hand a value to the control task and end the provisioning task.
///
/// The reply is a *promise*, not a confirmation: the store is written by the
/// control task at the top of its next tick, up to one control period later, and
/// this function has no way to know whether that write succeeded. The receipt is
/// the control task's log line — `config: a wifi credential from the console was
/// stored`, or the `error!` beside it — and it reaches the same UART0, so the
/// operator reads exactly one of them. Answering "ok applied" here would be
/// claiming something this function cannot observe.
///
/// **This does not reset the machine, and that is the fix, not an omission.** An
/// earlier revision staged the credential and then called `esp_restart()`
/// immediately, which is a race the operator loses every time: the control task
/// wakes every [`crate::main::CONTROL_PERIOD_MS`], so a reset issued microseconds after
/// `Handoff::stage` resets the machine *before* it has read the slot, and the
/// credential is silently lost with a success reply already on the console. The
/// reboot belongs to the control task, which owns the store and knows whether
/// the write worked; see the `handoff.take()` arm of `control_task`.
fn stage_and_stop(
    serial: &cc_hal_esp32::provisioning::Serial<'_>,
    handoff: &Handoff,
    staged: Staged,
    reply: &str,
) {
    if handoff.stage(staged) {
        info!("serial: staged and handed to the control task: {handoff:?}");
        serial.write_reply(&provisioning::line(reply));
    } else {
        serial.write_reply(&provisioning::line(
            "err an earlier command is still waiting to be applied — `wifi apply` first",
        ));
    }
}

/// How often the provisioning task polls UART0, in milliseconds.
///
/// 20 ms, which is 2.3 characters at 115200 baud — fast enough that a typed
/// command appears without a visible lag, slow enough that an idle console
/// costs 50 wake-ups a second rather than 1000.
pub const PROVISION_POLL_MS: u32 = 20;

/// How often the provisioning task says it is still waiting, in milliseconds.
///
/// 60 s. Long enough that it does not scroll a terminal away, short enough that
/// an operator who typed a command into the wrong window finds out.
pub const PROVISION_HEARTBEAT_MS: u32 = 60_000;

/// The stack of the provisioning task.
///
/// 4 KB. It holds a 112-byte parser and a 64-byte read buffer and formats a
/// `String` per reply; the smallest stack in this firmware that is not obviously
/// wrong.
pub const PROVISION_THREAD_STACK_BYTES: usize = 4 * 1024;
