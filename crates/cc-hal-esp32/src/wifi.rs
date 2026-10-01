//! Wi-Fi station mode: bring-up, the hostname ordering, and the monitor.
//!
//! Owner: **R3-12** (task B).
//!
//! # What this replaces
//!
//! `src/network/CleverCoffeeWiFiManager.cpp`, 260 lines, of which the
//! interesting parts are:
//!
//! * `configureWiFiManager` + `attemptConnection` — the tzapu `WiFiManager`
//!   captive portal. **Not ported here**; the reason is at the bottom of this
//!   file, and it is a decision rather than an omission.
//! * `checkAndMaintainConnection` — the retry / circuit-breaker / offline-mode
//!   decision. That is [`cc_domain::wifi::Monitor`], host-tested; this file
//!   performs the ESP-IDF call it asks for and nothing else.
//! * `getSignalStrength` — the four-bucket mapping. Also
//!   [`cc_domain::wifi::Signal`].
//!
//! # The hostname ordering, and why it is structural here
//!
//! `include/clevercoffee/network/WiFiStaConnect.h:13-30` exists for one
//! reason, stated in its own comment: *"Apply STA hostname before `WiFi.begin`
//! so DHCP client ID matches config"*. Setting it after the association starts
//! applies it to the *next* association, and the symptom is a machine whose
//! name changes when its DHCP lease renews, months later.
//!
//! In `esp-idf-svc` the ordering is not something to get right by discipline —
//! it is a property of the types. There is no public `set_hostname` on the
//! Wi-Fi driver: `EspNetif::set_hostname` is **private**
//! (`esp-idf-svc` 0.53.0 `src/netif.rs:624`, no `pub`). The hostname reaches the
//! netif only as `DHCPClientSettings::hostname` in the netif's construction
//! configuration, applied inside `EspNetif::new_with_conf` at
//! `netif.rs:387,489-490` — that is, when the netif is *created*, which is
//! necessarily before `wifi.start()` and therefore before any association. So
//! [`Sta::new`] cannot construct the wrong order, and
//! `cc_domain::wifi::ConnectionOrder::hostname_is_before_association` is a test
//! of the reasoning rather than a guard on a call sequence.
//!
//! It is still worth stating out loud, because the alternative is a future
//! editor reaching for the raw `esp_netif_set_hostname` — which is `unsafe`,
//! which this workspace denies, and which would be the wrong place anyway.

use alloc::format;
use alloc::string::{String, ToString};
use core::net::Ipv4Addr;

use cc_domain::wifi::{Action, ConnectionOrder, Monitor, Signal};
use esp_idf_hal::delay::FreeRtos;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::ipv4;
use esp_idf_svc::netif::{EspNetif, NetifConfiguration, NetifStack};
use esp_idf_svc::sys::{EspError, ESP_ERR_INVALID_ARG};
use esp_idf_svc::wifi::{
    AuthMethod, ClientConfiguration, Configuration, EspWifi, PmfConfiguration, WifiDriver,
};
use log::{info, warn};

use crate::time::now_ms;

/// The C++'s STA connect timeout, in milliseconds.
///
/// `CleverCoffeeWiFiManager.cpp:100` polls `WiFi.status()` for 10 s after
/// `beginStaWithHostname`, and `:70` sets `setConnectTimeout(10)` with the
/// comment *"using 10s to connect to WLAN, 5s is sometimes too short!"*. Both
/// are 10 s; this port uses one number for both.
pub const CONNECT_TIMEOUT_MS: u32 = 10_000;

/// How often the monitor is polled, in milliseconds.
///
/// The C++ runs `checkAndMaintainConnection` from the main loop, which is
/// continuous. Here the network task has its own cadence, deliberately slower
/// than the control tick: 1 s is far finer than the 10 s–5 min backoff
/// schedule needs, and a 400 ms poll would wake the task 2.5 times a second to
/// discover it had nothing to do.
pub const MONITOR_PERIOD_MS: u32 = 1_000;

/// Bring up the process-wide network stack: lwIP and the default event loop.
///
/// Called **unconditionally**, before the radio and before the HTTP server, and
/// the unconditional part is the whole point. `esp_netif_init` and
/// `esp_event_loop_create_default` are both once-per-process, and both are
/// needed by anything that opens a socket — including `esp_http_server`, which
/// does not care whether a radio exists.
///
/// That is a real crash this build had: with no SSID stored, the radio bring-up
/// was skipped, `esp_netif_init` was never called, and the first thing the HTTP
/// server did was `assert failed: tcpip_send_msg_wait_sem
/// tcpip.c:454 (Invalid mbox)`. lwIP's TCP/IP thread does not exist yet. A
/// machine with no network is a machine whose *console* and *API* still work.
///
/// # Errors
///
/// `EspError` if the netif stack or the default event loop cannot be created.
/// The caller treats this as fatal: without lwIP there is no HTTP server, no
/// MQTT and no telnet, which is the whole of this firmware's network tier.
pub fn init_stack() -> Result<EspSystemEventLoop, EspError> {
    NetifStack::initialize()?;
    EspSystemEventLoop::take()
}

/// A station interface, up and associating or not.
///
/// One type, because there is exactly one radio and one netif. "Not up yet" is
/// reported as a boolean and the monitor decides what to do about it, rather
/// than being a state every method has to handle.
pub struct Sta {
    wifi: EspWifi<'static>,
    monitor: Monitor,
    hostname: String,
}

impl Sta {
    /// Bring the station interface up with `hostname` already on the netif.
    ///
    /// # The ordering
    ///
    /// The `NetifConfiguration` built here is the first thing that touches the
    /// network stack, and it carries the hostname, so
    /// `esp_netif_set_hostname` runs inside `EspNetif::new_with_conf` before the
    /// Wi-Fi driver exists. By the time [`Sta::connect`] can call
    /// `esp_wifi_connect()`, the DHCP client identifier is fixed. That is the
    /// whole of the C++'s rule, and it is why `EspWifi::new` is not used: that
    /// constructor builds its own netif (`wifi.rs:1590-1592`) from
    /// `NetifStack::Sta`, with no hostname, which is exactly the bug the C++
    /// header warns about.
    ///
    /// `sys_loop` is the process-wide default event loop, created once by
    /// [`init_stack`] and shared with the HTTP server. It is a parameter rather
    /// than something this constructor takes, because `EspSystemEventLoop::take`
    /// is take-once (`eventloop.rs:499-512`) and a second `take` is
    /// `ESP_ERR_INVALID_STATE` — so *something* has to own the decision to call
    /// it, and the thing that knows the boot order is the startup sequence.
    ///
    /// # Errors
    ///
    /// Any `EspError` from netif or driver initialisation. Not recoverable and
    /// not worth recovering: every caller treats a failure as "the machine runs
    /// offline", because that is what a machine with no radio does.
    pub fn new(
        modem: esp_idf_hal::modem::Modem<'static>,
        hostname: &str,
        sys_loop: &EspSystemEventLoop,
    ) -> Result<Self, EspError> {
        let mut conf = NetifConfiguration::wifi_default_client();
        // THE ordering.
        conf.ip_configuration = Some(ipv4::Configuration::Client(
            ipv4::ClientConfiguration::DHCP(ipv4::DHCPClientSettings {
                hostname: Some(
                    heapless::String::try_from(hostname)
                        .map_err(|_| EspError::from_infallible::<ESP_ERR_INVALID_ARG>())?,
                ),
            }),
        ));
        let sta_netif = EspNetif::new_with_conf(&conf)?;
        let ap_netif = EspNetif::new(NetifStack::Ap)?;

        let driver = WifiDriver::new(modem, sys_loop.clone(), None)?;
        let mut wifi = EspWifi::wrap_all(driver, sta_netif, ap_netif)?;
        wifi.start()?;

        let this = Self {
            wifi,
            monitor: Monitor::new(),
            hostname: String::from(hostname),
        };
        this.log_ordering();
        Ok(this)
    }

    /// Log the ordering fact at boot.
    ///
    /// The C++ has this as a comment. This has it as a log line, because the
    /// ordering is invisible at runtime: a wrong hostname is a DHCP problem on a
    /// network the machine may not be on for weeks, and the only evidence will
    /// be whatever this line said at the boot it went wrong.
    #[allow(
        clippy::unused_self,
        reason = "a method on `Sta` so it can only be called once an interface \
                  exists; making it a free function would let it be called \
                  before the netif is built, which is the mistake it documents"
    )]
    fn log_ordering(&self) {
        let order = ConnectionOrder {
            netif_created_with_hostname: true,
            driver_started: true,
            association_requested: false,
            dhcp_requested: false,
        };
        assert!(
            order.hostname_is_before_association(),
            "the STA hostname must be on the netif before any association"
        );
        info!(
            "wifi: netif created with the hostname, then the driver started — the \
             DHCP client id is fixed before the first association"
        );
    }

    /// The configured hostname, for the log and for `/api/status`.
    ///
    /// The hostname is not a credential and is reported in the clear, unlike the
    /// SSID — see [`Sta::describe`].
    #[must_use]
    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    /// Whether the station is associated.
    ///
    /// `WifiDriver::is_connected` (`wifi.rs:741-753`) is "STA mode enabled and
    /// the STA status is `Connected`" — the association, not the DHCP lease.
    /// The C++'s `WiFi.status() == WL_CONNECTED` is the same test, and both are
    /// deliberately *not* "has an IP": the monitor's backoff must not treat a
    /// slow DHCP server as a failed association.
    #[must_use]
    pub fn is_associated(&self) -> bool {
        self.wifi.is_connected().unwrap_or(false)
    }

    /// Whether the netif has come up and taken an address.
    #[must_use]
    pub fn is_up(&self) -> bool {
        self.wifi.is_up().unwrap_or(false)
    }

    /// The station's IPv4 address, once DHCP has given it one.
    #[must_use]
    pub fn ip(&self) -> Option<Ipv4Addr> {
        self.wifi.sta_netif().get_ip_info().ok().map(|ip| ip.ip)
    }

    /// The received signal strength in dBm, or the C++'s -100 dBm stand-in.
    ///
    /// `CleverCoffeeWiFiManager.cpp:246`. Reproduced so the bucket arithmetic in
    /// [`cc_domain::wifi::Signal`] is the C++'s arithmetic rather than a
    /// slightly different one.
    #[must_use]
    pub fn rssi(&self) -> i32 {
        if self.is_associated() {
            self.wifi.get_rssi().unwrap_or(Signal::NOT_CONNECTED_RSSI)
        } else {
            Signal::NOT_CONNECTED_RSSI
        }
    }

    /// The 0–4 signal bucket, or [`Signal::Offline`] while offline mode is set.
    ///
    /// `CleverCoffeeWiFiManager::getSignalStrength`, including its refusal to
    /// report a signal at all while offline (`:234-236`).
    #[must_use]
    pub fn signal(&self) -> Signal {
        if self.monitor.is_offline() {
            Signal::Offline
        } else {
            Signal::from_rssi(self.rssi())
        }
    }

    /// Whether the machine has given up on Wi-Fi.
    #[must_use]
    pub fn is_offline(&self) -> bool {
        self.monitor.is_offline()
    }

    /// Start an association with `ssid` and `password`.
    ///
    /// An empty password means an open network, matching
    /// `beginStaWithHostname` (`WiFiStaConnect.h:22-26`), which branches on
    /// exactly this.
    ///
    /// # Errors
    ///
    /// `ESP_ERR_INVALID_ARG` for an SSID longer than 32 bytes or a password
    /// longer than 64 — the widths of `wifi_sta_config_t::ssid` and
    /// `::password`. Rejected here rather than truncated, because a truncated
    /// SSID associates with the *wrong network*.
    /// The auth mode and PMF the station offers.
    ///
    /// **Both defaults were wrong on this machine.** `ClientConfiguration`'s
    /// `auth_method` defaults to [`AuthMethod::WPA2Personal`] and its `pmf_cfg`
    /// to [`PmfConfiguration::NotCapable`], because that is what a
    /// *compile-time* default should be. Taken with `..Default::default()` — which
    /// is what this call used to do — the station announced **WPA2 only, no
    /// PMF**, and an AP that offers WPA3 would not associate: no `wifi:state:`
    /// transitions at all, a 10 s timeout, five retries, offline.
    ///
    /// The C++ does not have this problem: `WiFi.begin(ssid, password)`
    /// (`WiFiStaConnect.h:26-28`) leaves `wifi_authmode` at zero, which ESP-IDF
    /// reads as "accept whatever the AP offers".
    ///
    /// So the station says [`AuthMethod::WPA2WPA3Personal`] and advertises PMF
    /// as **capable and required** — which is what WPA3 needs, and which a WPA2
    /// AP tolerates, because PMF is negotiated rather than demanded by the peer.
    fn station_security() -> (AuthMethod, PmfConfiguration) {
        // **`WPA2Personal`, and the log is why.**
        //
        // ESP-IDF's `wifi_auth_mode_t` is a *sequence*, not a bitmask, and the
        // driver compares the AP's mode to ours for **equality** — it does not
        // accept a superset. The station said it would take WPA2/WPA3 and the
        // access point advertises WPA2-PSK, and the driver said so itself:
        //
        //     wifi:authmode threshold failure, ignore!, (recvd, thresh) : (3, 7)
        //
        // `3` is `WIFI_AUTH_WPA2_PSK`; `7` is what we asked for. A station
        // configured for "WPA2 or WPA3" therefore **cannot join a WPA2-only
        // network**, which is the opposite of what the name suggests — the fix
        // for a WPA2 machine is to name WPA2, not to widen it.
        //
        // PMF is advertised but not demanded. `required: true` is how a station
        // stops itself joining a WPA2-only AP that has no PMF.
        (
            AuthMethod::WPA2Personal,
            PmfConfiguration::Capable { required: false },
        )
    }

    /// Start an association with `ssid` and `password`.
    ///
    /// # Errors
    ///
    /// `ESP_ERR_INVALID_ARG` for an SSID longer than 32 bytes or a password
    /// longer than 64 — the widths of `wifi_sta_config_t::ssid` and `::password`.
    /// Rejected rather than truncated, because a truncated SSID associates with
    /// the *wrong network*, or with none.
    pub fn connect(&mut self, ssid: &str, password: &str) -> Result<(), EspError> {
        let (auth_method, pmf_cfg) = Self::station_security();
        let conf = Configuration::Client(ClientConfiguration {
            ssid: bounded(ssid)?,
            password: bounded(password)?,
            auth_method,
            pmf_cfg,
            ..Default::default()
        });
        self.wifi.set_configuration(&conf)?;
        self.wifi.connect()
    }

    /// Block until the station is associated, or [`CONNECT_TIMEOUT_MS`] passes.
    ///
    /// The C++'s 10 s poll at `CleverCoffeeWiFiManager.cpp:99-104`, at its
    /// `delay(100)` granularity.
    ///
    /// The only blocking call in the network tier, and it is bounded: it returns
    /// whether it succeeded and the caller decides. It runs on the network task,
    /// never on the control task (04 §3.2), so a 10 s wait here cannot delay a
    /// heater decision.
    ///
    /// Also — unlike the C++ — it needs no watchdog dance. The C++ wraps exactly
    /// this call in `disableLoopWDT()` (`:96-97`, and again at `:117`) because
    /// its watchdog is a 5 s task watchdog and its poll is 10 s. Here the single
    /// subscription belongs to the control task alone (04 §3.4), so the network
    /// task is not a subscriber and there is nothing to disable. That is worth
    /// naming, because "I removed the `disableLoopWDT`" otherwise reads as "I
    /// removed a safety net".
    pub fn wait_for_connection(&mut self) -> bool {
        let deadline = now_ms().wrapping_add(CONNECT_TIMEOUT_MS);
        while now_ms() < deadline {
            if self.is_associated() {
                return true;
            }
            FreeRtos::delay_ms(100);
        }
        self.is_associated()
    }

    /// Disconnect and associate again with the stored credentials.
    ///
    /// `reconnectStaWithHostname` (`WiFiStaConnect.h:32-37`) calls
    /// `WiFi.disconnect()`, `applyStaHostname()` and `WiFi.begin()`. Here the
    /// second step is a no-op, and it is a no-op for a reason: the hostname is a
    /// netif field, set at construction, and it survives a disconnect. That is
    /// [`Self::log_ordering`] restated at the point where the C++ would have had
    /// to re-apply it.
    ///
    /// # Errors
    ///
    /// `EspError` from `esp_wifi_disconnect` or `esp_wifi_connect`. Both are
    /// recoverable: the monitor records the failure and backs off.
    pub fn reconnect(&mut self) -> Result<(), EspError> {
        self.wifi.disconnect()?;
        self.connect_stored()
    }

    /// Associate with whatever [`Sta::connect`] last configured.
    ///
    /// `esp_wifi_connect()` with no re-configuration, which is what `WiFi.begin()`
    /// does in the C++'s `reconnectStaWithHostname`.
    ///
    /// # Errors
    ///
    /// `EspError` from `esp_wifi_connect`.
    pub fn connect_stored(&mut self) -> Result<(), EspError> {
        self.wifi.connect()
    }

    /// Forget the credentials the driver holds, so no further association is
    /// attempted. `POST /api/wifi/clear` and `wifi clear`.
    ///
    /// # Errors
    ///
    /// `EspError` from `esp_wifi_disconnect`.
    pub fn disconnect(&mut self) -> Result<(), EspError> {
        self.wifi.disconnect()
    }

    /// One iteration of the C++'s `checkAndMaintainConnection`, plus the ESP-IDF
    /// call it asks for.
    ///
    /// The decision is [`Monitor::poll`]'s; this performs it. Returns whether
    /// the machine is offline, which is the one thing a caller must react to
    /// differently.
    pub fn poll(&mut self) -> bool {
        let now = now_ms();
        let action = self.monitor.poll(self.is_associated(), now);
        match action {
            // `Connected` and `Wait` are the two "nothing to do" answers, and
            // they are the common ones: the monitor is polled every second and
            // the backoff makes `Wait` the normal state between attempts.
            Action::Connected | Action::Wait { .. } => {}
            Action::Retry {
                attempt,
                next_delay_ms,
            } => {
                info!("wifi: reconnect attempt {attempt}, next retry in {next_delay_ms} ms");
                if let Err(err) = self.connect_stored() {
                    warn!("wifi: reconnect attempt {attempt} failed to start: {err:?}");
                    self.monitor.record_attempt_result(false, now);
                }
            }
            Action::CircuitOpen => {
                warn!("wifi: circuit breaker open — not touching the radio");
            }
            Action::Offline => {
                warn!("wifi: max reconnection attempts reached — running offline");
            }
        }
        self.monitor.is_offline()
    }

    /// Bring the monitor back out of offline mode, as a reboot would.
    ///
    /// Called after `wifi apply`. See [`Monitor::set_offline`] for why this
    /// resets the policies too.
    pub fn leave_offline(&mut self) {
        self.monitor.set_offline(false);
    }

    /// A one-line description of the link that contains no credential.
    ///
    /// The SSID is deliberately absent. The C++ logs it on every attempt
    /// (`:96-98`, `:213`, `:221`) and shows it on the OLED (`:135-136`); this
    /// port does not, for the same reason it never logs the Wi-Fi password: a
    /// support log is a place credentials end up, and the SSID is half of a
    /// credential pair.
    #[must_use]
    pub fn describe(&self) -> String {
        let ip = self
            .ip()
            .map_or_else(|| "none".to_string(), |v| format!("{v}"));
        format!(
            "sta: associated={} up={} ip={ip} rssi={} dBm signal={}/4 offline={} \
             reconnects={}",
            self.is_associated(),
            self.is_up(),
            self.rssi(),
            self.signal().as_bars(),
            self.is_offline(),
            self.monitor.reconnect_attempts(),
        )
    }
}

/// Convert a `&str` into the `heapless::String<N>` the `embedded-svc`
/// configuration types use, refusing rather than truncating.
///
/// `set_str_no_termination_requirement` (`wifi.rs:196-203`) already returns
/// `ESP_ERR_INVALID_ARG` for an over-long SSID or password, so a truncating
/// conversion here would only hide *which* field was wrong. The widths are the
/// ESP-IDF ones: 32 for an SSID, 64 for a passphrase.
fn bounded<const N: usize>(value: &str) -> Result<heapless::String<N>, EspError> {
    heapless::String::try_from(value)
        .map_err(|_| EspError::from_infallible::<ESP_ERR_INVALID_ARG>())
}

// ---------------------------------------------------------------------------
// The captive portal: evaluated and deferred
// ---------------------------------------------------------------------------
//
// `configureWiFiManager` and the portal half of `attemptConnection`
// (`CleverCoffeeWiFiManager.cpp:64-77, 110-127`) are **not** ported here, and the
// reason is a decision, not an omission.
//
// * **It is redundant with what is here.** The UART protocol is proven on this
//   hardware (08 §5.2, recovered from a real image), and a portal needs a
//   browser, which needs a working Wi-Fi interface — the thing being
//   provisioned. The portal is the fallback for "you cannot open a terminal",
//   not for "you cannot open a terminal *and* have no cable".
// * **It costs heap this firmware does not have.** A softAP plus a DNS
//   interceptor plus an HTTP form, concurrently with the control loop, the REST
//   server, MQTT and the telnet stream, is the same arithmetic that produced
//   ADR-0002's OOM abort. 02 §6 records that every available Rust
//   captive-portal crate binds port 80 itself, so the portal and the REST API
//   could not share it anyway.
// * **The plan defers it too.** 05 §5 P1 vs P2, and the recovered oracle that
//   did both carried a `littlefs` partition and an embedded React bundle that
//   this firmware does not yet have.
//
// It is sequenced, not dropped: `POST /api/wifi` and `POST /api/wifi/clear`
// (08 §5.1) already expose the same two operations over REST, so a machine that
// *is* on the network can be re-provisioned without the portal. What a portal
// adds is only the first-contact case, and that is the case the UART line
// covers.

#[cfg(any(test, feature = "device-tests"))]
#[cfg_attr(feature = "device-tests", doc(hidden))]
pub mod tests {
    // A unit-test module globs its parent on purpose: the cases are exercising
    // the parent's private helpers, which is the point of keeping them in the
    // same file. `clippy::wildcard_imports` normally makes an exception for
    // `use super::*` inside a `#[cfg(test)]` module, and this module is
    // `#[cfg(any(test, feature = "device-tests"))]` -- the on-target runner
    // compiles it outside a test build -- so the exception no longer applies and
    // the allowance is made explicitly here instead of in eight import lists
    // that would rot.
    #![allow(clippy::wildcard_imports)]

    use super::*;

    #[cfg_attr(test, test)]
    pub fn the_connect_timeout_is_the_cpp_ten_seconds() {
        // CleverCoffeeWiFiManager.cpp:70 and :100 both use 10 s, with the
        // comment "5s is sometimes too short!". A test because it is the kind of
        // number that gets tidied to a round 5.
        assert_eq!(CONNECT_TIMEOUT_MS, 10_000);
    }

    #[cfg_attr(test, test)]
    pub fn the_monitor_poll_is_slower_than_the_control_tick() {
        // Set below the 400 ms control tick, this would wake the network task
        // more often than the control loop for no benefit.
        const { assert!(MONITOR_PERIOD_MS >= 1_000) };
    }

    #[cfg_attr(test, test)]
    pub fn an_over_long_ssid_is_refused_rather_than_truncated() {
        // Truncating an SSID associates with the *wrong network*, which is a
        // far worse failure than a rejected configuration.
        assert!(bounded::<32>(&"s".repeat(32)).is_ok());
        assert!(bounded::<32>(&"s".repeat(33)).is_err());
        assert!(bounded::<64>(&"p".repeat(64)).is_ok());
        assert!(bounded::<64>(&"p".repeat(65)).is_err());
    }
}
