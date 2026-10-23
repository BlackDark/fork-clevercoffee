//! Panel owner, rendering the latest [`FrameRequest`](crate::slots::FrameRequest).
//!
//! A frame takes tens of milliseconds on the shared I²C bus, so rendering runs
//! outside the 10 ms control task. Refresh is limited by
//! [`cc_hal_esp32::display::REFRESH_INTERVAL_MS`].
//! Boot first shows the version, then the IP once it appears in the telemetry snapshot.

use std::sync::Arc;

use cc_display::display::Rotation;
use cc_display::templates::TemplateId;
use cc_hal_esp32::display_shared::SharedPanel;
use cc_hal_esp32::time::now_ms;
use esp_idf_hal::delay::FreeRtos;
use log::info;

use crate::slots::FrameSlot;

/// The panel's own refresh interval.
pub const REFRESH_MS: u32 = cc_hal_esp32::display::REFRESH_INTERVAL_MS;

/// How often the panel's counters are logged.
const REPORT_INTERVAL_MS: u32 = 60_000;

/// The display task's state: the panel, the scratch framebuffer, and the two
/// one-shot boot screens.
pub struct DisplayTask {
    /// The panel, or `None` when no display is fitted.
    panel: Option<SharedPanel<'static>>,
    /// The 1 KB framebuffer, allocated once.
    ///
    /// Not a local: the control task's stack is 8 KB and this is 1 KB of it
    /// (`refresh_display`'s comment records the stack overflow that a local
    /// caused on the first frame).
    scratch: Box<cc_display::display::Display>,
    /// The template the machine is configured for.
    template: TemplateId,
    /// Where the control task's frame requests arrive.
    frame: Arc<FrameSlot>,
    /// The telemetry snapshot, for the Wi-Fi address on the boot screen.
    shared: Arc<cc_net_esp32::web::Shared>,
    /// The brew-timer state of the last frame drawn, so a transition can be
    /// announced rather than the state merely repeated.
    last_brew_timer: cc_display::model::BrewTimerState,
}

impl DisplayTask {
    /// Take the panel and the channels.
    #[must_use]
    pub fn new(
        panel: Option<SharedPanel<'static>>,
        template: TemplateId,
        frame: Arc<FrameSlot>,
        shared: Arc<cc_net_esp32::web::Shared>,
    ) -> Self {
        Self {
            panel,
            scratch: Box::new(cc_display::display::Display::new()),
            template,
            frame,
            shared,
            last_brew_timer: cc_display::model::BrewTimerState::Idle,
        }
    }

    /// Draw one of the two boot screens and push it, ignoring the refresh gate.
    fn boot_screen(&mut self, line1: &str, line2: &str) {
        let Some(panel) = self.panel.as_mut() else {
            return;
        };
        self.scratch
            .set_display_rotation(if self.template.is_upright() {
                Rotation::R1
            } else {
                Rotation::R0
            });
        cc_display::boot::draw(&mut self.scratch, line1, line2, self.template);
        // `refresh_now`, not `refresh`: a boot screen that lost a race against
        // the 100 ms gate would never appear, which is the "the startup screen
        // is missing" defect. These are one-shot frames on a task that has drawn
        // nothing yet, so the gate has nothing to protect.
        let _ = panel.refresh_now(self.scratch.framebuffer().as_bytes());
    }

    /// The loop.
    pub fn run(mut self) -> ! {
        // The panel's own counters, once a minute, for the reason the control
        // task used to log them: a display that has stopped updating is
        // invisible from the outside except by looking at the machine, and
        // `frames=` not advancing is the one number that says so without anyone
        // having to notice. It belongs here now because the panel does.
        let mut last_report = 0_u32;
        let version = env!("CARGO_PKG_VERSION");
        let (version_line1, version_line2) = cc_display::boot::text::version(version);
        let mut shown_version = false;
        let mut shown_address = false;

        loop {
            if !shown_version {
                info!("display: boot screen — {version_line2}");
                self.boot_screen(version_line1, version_line2);
                shown_version = true;
            }

            // The address screen, the moment the radio has a **real** address.
            //
            // Not "the moment it has an `Option`": the snapshot publishes an IP
            // as soon as the station is associated, which is *before* DHCP has
            // answered, and that IP is `0.0.0.0`. The screen was therefore drawn
            // with `0.0.0.0` on it — which is the one thing the screen exists to
            // prevent, and the failure
            // `docs/operations/runbook.md` calls out as a
            // failed check. Wait for an address that is not unspecified.
            if !shown_address {
                // `Telemetry::ip` is the **formatted** address, so "unspecified"
                // is the literal `0.0.0.0` — and the snapshot publishes one as
                // soon as the station associates, which is before DHCP answers.
                let address = self.shared.snapshot().ip.filter(|ip| ip != "0.0.0.0");
                if let Some(ip) = address {
                    let ip = ip.clone();
                    let (line1, line2) = cc_display::boot::text::wifi_connected(&ip);
                    info!("display: wifi screen — {ip}");
                    self.boot_screen(line1, line2);
                    shown_address = true;
                    // Give the address a moment on its own before the first normal frame
                    // overwrites it. Wi-Fi setup blocks for seconds before this point, so
                    // the logo is what an operator watches; one second is the shortest
                    // wait that makes the address readable rather than a flash.
                    FreeRtos::delay_ms(1_000);
                }
            }

            // One frame, if there is one and the panel is not blanked. A
            // `None` frame means the control task has not published yet, which
            // is boot: the panel then keeps whatever the boot screens drew,
            // rather than being shown a frame of zeroes.
            if let Some(request) = self.frame.frame() {
                // The brew timer is the one thing on this screen that is not
                // visible in any log line, and it is a state machine the control
                // task advances. Announcing each transition makes it provable
                // from the console: a brew logs `Idle -> Running` when the pump
                // starts and `Running -> PostBrew` when it stops, with the timer
                // value at both.
                if request.input.brew_timer != self.last_brew_timer {
                    info!(
                        "display: brew timer {:?} -> {:?} at {:.1} s",
                        self.last_brew_timer,
                        request.input.brew_timer,
                        request.input.brew_time_ms / 1000.0
                    );
                    self.last_brew_timer = request.input.brew_timer;
                }
                if let Some(panel) = self.panel.as_mut() {
                    panel.set_blank(request.blank);
                    if !request.blank {
                        let _ = cc_display::templates::render(
                            self.template,
                            &mut self.scratch,
                            &request.input,
                            &request.config,
                        );
                        let _ = panel.refresh(self.scratch.framebuffer().as_bytes(), now_ms());
                    }
                }
            }
            if now_ms().wrapping_sub(last_report) >= REPORT_INTERVAL_MS {
                last_report = now_ms();
                if let Some(panel) = self.panel.as_ref() {
                    info!("{}", panel.report());
                }
            }
            FreeRtos::delay_ms(REFRESH_MS);
        }
    }
}

/// The `cc_display` template for a configured
/// [`DisplayTemplate`](cc_domain::system::DisplayTemplate).
///
/// Two crates' enums with the same six layouts in the same order; nothing keeps
/// them in step, so the mapping is written out rather than cast.
pub(crate) fn template_for(
    configured: cc_domain::system::DisplayTemplate,
) -> cc_display::templates::TemplateId {
    use cc_domain::system::DisplayTemplate;
    match configured {
        DisplayTemplate::Standard => TemplateId::Standard,
        DisplayTemplate::Minimal => TemplateId::Minimal,
        DisplayTemplate::TemperatureOnly => TemplateId::TemperatureOnly,
        DisplayTemplate::Scale => TemplateId::Scale,
        DisplayTemplate::Upright => TemplateId::Upright,
        DisplayTemplate::Modern => TemplateId::Modern,
    }
}

/// The `cc_display` view of the machine's configuration.
///
/// `cc_display::model::Config` is deliberately a *separate* type from
/// `cc_config::Config`: the display crate is a pure renderer with 48 goldens and
/// a pixel-for-pixel comparison against real U8g2, and it has no business knowing about
/// Wi-Fi credentials or the NVS blob. The consequence is that this mapping
/// exists, and it is the seam where a new display-relevant parameter has to be
/// wired up by hand — which is the point.
///
/// **Every field is listed, with no `..Default::default()`.** That is what makes
/// the seam load-bearing: a field added to the display crate's `Config` is a
/// compile error here until someone decides what the machine's setting means for
/// it, rather than a template that silently keeps using the default. The goldens
/// can afford `..default()` because they are fixtures; this is not a fixture.
pub(crate) fn display_config(config: &cc_config::Config) -> cc_display::model::Config {
    use cc_display::model::{BrewMode, Config as DisplayConfig, Language, ScaleType};
    use cc_domain::system::DisplayTemplate;

    DisplayConfig {
        brew_switch_enabled: config.hardware.switches.brew.enabled,
        scale_enabled: config.hardware.sensors.scale.enabled,
        // The display crate's `ScaleType` is a two-way question (wired load
        // cell or radio) where the config's is three (one cell, two cells, or
        // radio). Single and dual are the same *layout* to a template, so they
        // collapse here; which one it is belongs to the driver, not the screen.
        scale_type: match config.hardware.sensors.scale.r#type {
            cc_domain::hardware::ScaleType::Bluetooth => ScaleType::Bluetooth,
            cc_domain::hardware::ScaleType::Hx711Dual
            | cc_domain::hardware::ScaleType::Hx711Single => ScaleType::Hx711,
        },
        pressure_enabled: config.hardware.sensors.pressure.enabled,
        oled_enabled: config.hardware.oled.enabled,
        upright_template: config.display.template == DisplayTemplate::Upright,
        inverted: config.display.inverted,
        language: match config.display.language {
            cc_domain::system::Language::English => Language::English,
            cc_domain::system::Language::German => Language::German,
            cc_domain::system::Language::Spanish => Language::Spanish,
        },
        heating_logo: u8::from(config.display.heating_logo),
        pid_off_logo: u8::from(config.display.pid_off_logo),
        fullscreen_brew_timer: config.display.fullscreen_brew_timer,
        fullscreen_manual_flush_timer: config.display.fullscreen_manual_flush_timer,
        fullscreen_hot_water_timer: config.display.fullscreen_hot_water_timer,
        post_brew_timer_duration_s: config.display.post_brew_timer_duration,
        blinking_delta: config.display.blinking.delta,
        backflush_reminder_enabled: config.maintenance.backflush_reminder.enabled,
        brew_mode: match config.brew.mode {
            cc_domain::process::BrewMode::Manual => BrewMode::Manual,
            cc_domain::process::BrewMode::Automatic => BrewMode::Automatic,
        },
        brew_by_time_enabled: config.brew.by_time.enabled,
        brew_by_weight_enabled: config.brew.by_weight.enabled,
        brew_by_weight_target: config.brew.by_weight.target_weight,
        mqtt_enabled: config.mqtt.enabled,
        // `cycles` is an i32 in the config and a u8 on screen. `unwrap_or` rather than
        // a cast: a machine configured with a nonsense value should render the default
        // layout detail, not wrap to 255 cycles.
        backflush_cycles: u8::try_from(config.backflush.cycles).unwrap_or(5),
    }
}
