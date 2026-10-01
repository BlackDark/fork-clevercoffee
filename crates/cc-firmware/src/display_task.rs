//! The display task: the panel, on the panel's cadence.
//!
//! Owner: **R4-01b** (the event-driven control loop).
//!
//! # What it owns
//!
//! The panel and nothing else. It renders whatever
//! [`FrameRequest`](crate::slots::FrameRequest) the control task last published,
//! at the panel's own refresh interval, and it draws the two boot screens before
//! any normal frame.
//!
//! # Why the panel left the control task
//!
//! Because of what a frame costs. The 1 KB framebuffer goes out as eight I²C
//! writes, which is tens of milliseconds of bus time on a 400 kHz bus — and it
//! was being paid **inside the control tick**, on the same task, in the same
//! iteration as the heater decision. That is the whole of the "the control tick
//! overruns its 10 ms budget in 62 % of ticks" finding in the migration notes:
//! the loop was not slow because the loop was slow, it was slow because the
//! display was in it.
//!
//! With the panel here, the control task's cost is the reducer and the applier,
//! and the panel's cost is the panel. The two contend for one I²C bus, which is
//! a mutex, and neither can make the other miss a deadline that matters: the
//! display may be 100 ms late, and the control decision may not be 10 ms late.
//!
//! # The refresh interval
//!
//! [`cc_hal_esp32::display::REFRESH_INTERVAL_MS`] = 100 ms, the C++'s
//! `DISPLAY_REFRESH_INTERVAL_MS` (`constants/Timing.h:38`). It is also the floor
//! on how quickly a screen change becomes visible, which is the C++'s floor
//! too: `LoopManager::updateDisplay` runs the display on the same loop as
//! everything else and the panel's own driver rate-limits the flush.
//!
//! # The boot screens
//!
//! `displayLogo` (`DisplayWidgets.h:404-446`) is called twice during the C++'s
//! boot — with the version (`SystemInitializer.cpp:134`) and then with the Wi-Fi
//! address (`:848`). It is drawn here because this task owns the panel, and the
//! address is not known to the control task: the radio publishes it into the
//! telemetry snapshot, so the second screen is drawn when the snapshot's IP
//! first appears.

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
    shared: Arc<cc_hal_esp32::web::Shared>,
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
        shared: Arc<cc_hal_esp32::web::Shared>,
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
            // prevent, and the failure `docs/integration-tests.md` calls out as a
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
                    // Give the address a moment on its own before the first
                    // normal frame overwrites it. The C++ has no such pause —
                    // its first loop simply draws over the logo — but its Wi-Fi
                    // setup blocks for seconds, so in practice the logo is the
                    // only thing an operator sees. One second is the shortest
                    // wait that makes the screen readable rather than a flash.
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
