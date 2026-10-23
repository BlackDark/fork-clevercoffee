//! Network bring-up and the stored-configuration safety check.
//!
//! `cc-config` and `cc-safety` cannot depend on each other, so firmware joins them
//! through [`cc_machine::safety_config`]. Rejected blobs remain stored for
//! diagnosis through `/api/nvs-debug`; defaults run instead and rejection is logged.
//!
//! Network code cannot actuate outputs. Only the control task subscribes to the
//! watchdog and opens the heater gate.
//! Control runs every 10 ms, network monitoring every second, and provisioning
//! polls every 50 ms while armed. HTTP and MQTT use ESP-IDF's own tasks.

use core::fmt::Write as _;
use std::sync::{Arc, Mutex};

use cc_config::blob_store::BlobConfigStore;
use cc_config::predecessor::{startup_notice, PredecessorProbe};
use cc_config::{Config, NVS_NAMESPACE};
use cc_hal_esp32::heap::{free_heap, min_free_heap};
use cc_hal_esp32::nvs::EspNvsBlob;
use cc_hal_esp32::time::now_ms;
use cc_net::web::{temperatures_json, Command};
use cc_net_esp32::provisioning::{self, Action, Session};
use cc_net_esp32::web::{Shared, Sse, Web};
use cc_net_esp32::wifi::Sta;
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
    /// `BlobConfigStore::load`/`save` take `&mut self`, so the store has one owner
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

    // Only when **this** firmware's store came up empty: a machine that already has a
    // Rust configuration has nothing to be told, and walking a namespace nobody will
    // read on every boot would be work for no possible output. The words and the
    // decision are `cc_config::predecessor`'s and are host-tested; the enumeration is
    // `cc_hal_esp32::nvs::probe_predecessor`'s, and it cannot fail — a probe that could
    // stop the firmware booting would be a worse fault than the one it reports.
    let predecessor = if stored.is_some() {
        PredecessorProbe::Skipped
    } else {
        cc_hal_esp32::nvs::probe_predecessor(store.backend())
    };
    if let Some(notice) = startup_notice(predecessor) {
        warn!("config: {notice}");
    }

    // The fail-closed rule. `cc_safety::load_or_default` is the whole of the
    // decision and is fully tested in `cc-safety`; this is the call.
    // **One construction, one owner.** This used to build `SafetyConfig` field by
    // field, separately from the control task's construction, and a reviewer's finding
    // was that the two drifted: this copy hard-coded the two relay trigger types
    // to `HighTrigger`, so `validate_config` never saw a stored low-trigger pump
    // or valve, the fail-closed rule did not discard the configuration — and
    // `main.rs` then applied that very polarity to the pins, energising the pump
    // at boot. Two constructions of a safety input is one too many.
    let safety = stored.as_ref().map(cc_machine::safety_config);

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
            // Finding #12: revert implicated keys and write them back. Do not
            // wipe all 98 parameters. If repair fails, full defaults plus the
            // Wi-Fi credential: reachable, and never running a refused config.
            warn!(
                "config: (configuration is unsafe to run: {violation:?}) -> \
                 repairing the implicated key(s) and keeping everything else"
            );
            let mut repaired = config.clone();
            let repair = crate::config_io::repair_unsafe(&mut repaired);
            if repair.resolved {
                for key in &repair.reverted {
                    info!("config: reverted {key} to its default");
                }
                match store.save(&repaired) {
                    Ok(()) => info!(
                        "config: the repair is persisted ({})",
                        repair.reverted.join(", ")
                    ),
                    Err(err) => warn!("config: could not persist the repair: {err}"),
                }
                config = repaired;
            } else {
                // Repair failed. Full defaults. Wi-Fi credential kept.
                warn!(
                    "config: the repair did not resolve it after {} pass(es) -> \
                     falling back to the compiled-in defaults",
                    repair.reverted.len()
                );
                let credential = config.system.wifi.clone();
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
    }

    // Described *after* every write above, so `/api/nvs-debug` cannot report a
    // blob that is not there yet. Read once: `BlobConfigStore::describe` reads the
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
        // Blob was unsafe. Repair kept the rest. Not "DISCARDED".
        ConfigOrigin::DiscardedUnsafe(_) => "stored but unsafe",
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
    /// The one OTA update in flight. Owned here so the control task can read it
    /// without reaching through [`crate::web::Web`], and so it outlives the
    /// server handle: a restart request raised by the last upload must still be
    /// seen by the tick that acts on it.
    pub ota: Arc<cc_net_esp32::ota::Session>,
    /// The SSE counters.
    pub sse: Arc<Sse>,
}

impl Network {
    /// A fresh, unstarted network surface.
    #[must_use]
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared::new()),
            ota: Arc::new(cc_net_esp32::ota::Session::new()),
            sse: Arc::new(Sse::new(cc_net_esp32::web::SseMode::Chunked)),
        }
    }
}

impl Default for Network {
    fn default() -> Self {
        Self::new()
    }
}

/// Keep the HTTP handle on its constructing thread: `EspHttpServer` is neither
/// `Send` nor `Sync`. The startup thread joins control for the process lifetime,
/// so a local handle stays alive without leaking it. Handlers are static `Send`
/// closures over shared values.
///
/// # Errors
///
/// Returns [`EspError`] on creation or registration, including handler-budget overflow.
pub fn start_http(
    network: &Network,
    config: &Config,
    nvs_description: &str,
    commands: Arc<cc_hal_esp32::task::CommandQueue>,
    parameters: &Arc<cc_hal_esp32::task::ParameterHandoff>,
) -> Result<Web, EspError> {
    let shared = Arc::clone(&network.shared);
    let session = Arc::clone(&network.ota);
    let sse = Arc::clone(&network.sse);
    let config = Arc::new(config.clone());
    // Non-blocking, drop-newest. A full queue means the control task is behind, which
    // is a condition to shed, not to report.
    let sink: Arc<dyn Fn(Command) + Send + Sync + 'static> = Arc::new(move |command: Command| {
        let _ = commands.try_send(command);
    });
    let web = Web::start(
        shared,
        &session,
        sse,
        &config,
        nvs_description,
        &sink,
        parameters,
    )?;
    info!(
        "http: {} routes registered; the large-response floor is {} B",
        cc_net_esp32::web::routes().len(),
        cc_net::web::HEAP_FLOOR_BYTES
    );
    Ok(web)
}

/// Update radio fields while preserving control's telemetry fields.
///
/// Both publishers must run sequentially on control because snapshot replacement
/// is whole-slot. Gate radio FFI reads on the one-second poll, not every 10 ms tick.
pub fn publish_radio(shared: &Shared, sta: Option<&Sta>) {
    // Read-modify-write, so this cannot be a bare `set`. Both publishers are on
    // the control task and are sequential, which is what makes the
    // read-copy-write here sound: no other task ever writes this slot.
    let mut slot = shared.snapshot();
    if let Some(sta) = sta {
        slot.signal = sta.signal().as_bars();
        slot.wifi_associated = sta.is_associated();
        slot.wifi_offline = sta.is_offline();
        // `heapless::String<15>` is exactly the longest a dotted-quad IPv4
        // address can be, so this never fails and never allocates. A longer
        // value would be `None`, which is the honest answer for "no address"
        // anyway — but `Sta::ip()` returns `Ipv4Addr`, so it cannot happen.
        // Formatted **straight into** the fixed-size buffer: `heapless` 0.9 has
        // no `TryFrom<String>` (and taking one would allocate), so this is
        // `core::fmt::Write` into the destination. `Ipv4Addr`'s `Display` is
        // dotted-quad, at most 15 characters, so it cannot overflow -- but if it
        // ever did, the buffer would hold a partial address, which is worse than
        // none, so the result is checked rather than assumed.
        slot.ip = sta.ip().and_then(|ip| {
            let mut text = heapless::String::<15>::new();
            write!(text, "{ip}").ok().map(|()| text)
        });
    } else {
        // No radio at all. Reporting "not associated, no address" is the truth,
        // and is what a machine that never provisioned a network should say.
        slot.signal = 0;
        slot.wifi_associated = false;
        slot.wifi_offline = false;
        slot.ip = None;
    }
    shared.publish(slot);
}

/// Publish the SSE `new_temps` event, if an HTTP server is running.
///
/// Called from the control task at the SSE interval (1 s). It reads the
/// snapshot the control task has *just* published, so the frame cannot disagree with
/// `/api/status`.
///
/// The **producer** is the control task and the **transport** is the event source: this
/// function produces, and the broadcaster task `Web::start` spawned delivers. The cadence
/// therefore lives in the control task and not inside the stream.
pub fn broadcast_temps(network: &Network) {
    let snapshot = network.shared.snapshot();
    network
        .sse
        .broadcast(Sse::frame("new_temps", &temperatures_json(&snapshot)));
    network.sse.note_push_attempt();
}

/// Log the heap once a minute: free, the minimum ever seen, and the large-response floor
/// with the two counters that show whether it is being respected.
pub fn log_heap_once_a_minute(network: &Network) {
    info!(
        "heap: {} B free, {} B minimum — the large-response floor is {} B \
         ({} served, {} refused)",
        free_heap(),
        min_free_heap(),
        cc_net::web::HEAP_FLOOR_BYTES,
        network.shared.large_responses(),
        network.shared.large_refused(),
    );
}

/// What the provisioning task captured, waiting for the control task to store it.
///
/// A credential is **not** a [`cc_net_esp32::web::Command`]. Those are `Copy` and eight
/// bytes because they cross a task queue, and a `String` cannot. That constraint is why a
/// credential takes this route instead, and it is a good one: a credential that could
/// travel as a command would be a credential an HTTP handler could set.
pub enum Staged {
    /// `wifi set <ssid>`, the password line, then `wifi apply`.
    Set(cc_net_esp32::provisioning::Pending),
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
/// The store has exactly one owner — the control task — and `BlobConfigStore::load`
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

/// Persist staged credentials on the store-owning control task.
///
/// Load/modify/save is one atomic blob update; an undecodable blob uses defaults.
/// Nested serde needs about 8.6 KB of stack; `bring_up::CONTROL_STACK_BYTES`
/// reserves 16 KB. Exhaustion can reset silently before the panic handler runs.
/// Measured load/save took 15/66 ms, 85 ms total: longer than one control period
/// but bounded within the five-second watchdog timeout.
pub fn apply_staged(
    store: &mut BlobConfigStore<EspNvsBlob>,
    staged: Staged,
) -> Result<(), cc_config::StoreError> {
    let mut config = store.load()?.unwrap_or_default();
    // Repair before save. An unsafe blob plus a new credential would otherwise
    // be written back whole, and the next boot would drop the credential.
    // Finding #12.
    let repair = crate::config_io::repair_unsafe(&mut config);
    if repair.resolved {
        for key in &repair.reverted {
            info!("config: reverted {key} to its default before writing the credential");
        }
    } else {
        warn!(
            "config: the stored configuration is unsafe and the repair could not \
             resolve it; writing the credential into a repaired-from-scratch one so \
             the machine stays reachable"
        );
        config = Config::default();
    }
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
/// The task ends by *returning* once a command has been handed to the control task, and
/// it is the control task that stores the credential and resets the machine, because the
/// control task owns the store. Resetting from this task instead would reset the machine
/// before the write, every time.
#[allow(
    clippy::needless_pass_by_value,
    reason = "the `Serial` IS the task: it owns UART0 for the task's whole life \
              and is never handed anywhere else. A `&mut` would suggest it \
              could be shared, which is the one thing a UART must not be."
)]
pub fn run_provisioning(serial: cc_net_esp32::provisioning::Serial<'_>, handoff: Handoff) {
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
            // `cc_net_esp32::provisioning::reply_text` puts the `CCWIFI `
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
                // The provisioning task exits after success. After a refusal the machine
                // is in the same unprovisioned state it was in, so the operator re-boots
                // when they are ready.
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
/// wakes every [`crate::control_task::CONTROL_PERIOD_MS`], so a reset issued microseconds after
/// `Handoff::stage` resets the machine *before* it has read the slot, and the
/// credential is silently lost with a success reply already on the console. The
/// reboot belongs to the control task, which owns the store and knows whether
/// the write worked; see `ControlLoop::take_console_credential`.
fn stage_and_stop(
    serial: &cc_net_esp32::provisioning::Serial<'_>,
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
