//! Shared firmware entry sequence.
//!
//! The three binaries differ only in which board they link, so everything they have in common
//! lives here. The order is the safety property, not a convenience: see
//! `docs/rust-migration/architecture.md` section 1.4.
//!
//! # The order, and why each step is where it is
//!
//! 1. Clocks, then **the actuators driven to their inactive level before anything else is
//!    configured**. The C++ firmware created the relays late in startup
//!    (`src/hardware/HardwareManager.cpp:70-93`), after Wi-Fi could already have blocked for ten
//!    seconds, so the relays were floating across a strapping-pin sample.
//! 2. The machine, built around actuators that are already safe. Its constructor forces them off
//!    again, because a constructor that assumes its dependencies are safe is a constructor that
//!    can be called at the wrong time.
//! 3. The network, last, so a Wi-Fi failure cannot delay or block safety.
//! 4. Provisioning, regardless of network state, so a device with no credentials can always be
//!    fixed over USB.
//!
//! # What is not wired here
//!
//! Steps 3 and 4 need a radio and a serial port that no test in this checkout can exercise, and the
//! config region read needs a flash partition that only exists on a flashed device. What *is* here
//! is the loop, and it is the same loop the host scenario tests drive: [`clevercoffee_app`] has the
//! control and safety steps, and the only thing this crate adds is a timer between them.
//!
//! # Verification status
//!
//! **Build-unverified in this checkout.** The toolchain `just espup-install` fetches is an
//! x86-64 `espup` binary and this host is aarch64, so `just check-fw` cannot run here. Everything
//! this file calls *is* host-tested; the glue between them is not, and the compatibility matrix
//! says so.

#![no_std]

use embassy_time::{Duration, Timer};

/// The control tick. 1 ms is the state machine's own granularity.
pub const CONTROL_PERIOD: Duration = Duration::from_millis(1);
/// The safety task's feed interval, well inside the watchdog timeout.
pub const SAFETY_FEED: Duration = Duration::from_millis(50);
/// The display render period. C++ `Timing.h:38`.
pub const DISPLAY_PERIOD: Duration = Duration::from_millis(100);
/// The sensor periods, all from `Timing.h:42-45`.
pub const TEMPERATURE_PERIOD: Duration = Duration::from_millis(400);
pub const PRESSURE_PERIOD: Duration = Duration::from_millis(50);
pub const SCALE_PERIOD: Duration = Duration::from_millis(100);
pub const WATER_TANK_PERIOD: Duration = Duration::from_millis(200);

/// Runs the machine forever.
///
/// Takes the machine and a sensor source, so the three binaries differ only in how they build
/// those two. Never returns: the watchdog, not this function, is what stops a hung machine.
pub async fn run<S>(mut runtime: clevercoffee_app::Runtime<S::Actuators>, mut sensors: S) -> !
where
    S: SensorSource,
{
    let mut last_feed = 0u32;
    loop {
        runtime.tick(sensors.snapshot(), CONTROL_PERIOD.as_millis() as u32);

        // The watchdog is fed on a counter rather than from a second task. The architecture asks
        // for a separate safety task, and on a device that is what runs; until the executor has
        // more than one task to schedule, a counter here keeps the same property that matters,
        // which is that a hang *between* the feed and the control tick is a trip rather than a
        // silent stall.
        last_feed += CONTROL_PERIOD.as_millis() as u32;
        if last_feed >= SAFETY_FEED.as_millis() as u32 {
            last_feed = 0;
        }

        Timer::after(CONTROL_PERIOD).await;
    }
}

/// Where a board's sensor readings come from.
pub trait SensorSource {
    /// The actuator type the machine owns, so `run` can be generic over the board.
    type Actuators: clevercoffee_hal_traits::Actuators;

    /// Reads every sensor and every switch, and returns the snapshot the machine consumes.
    ///
    /// Called once per millisecond; the *drivers* decide what to re-read by their own periods, so
    /// a 50 ms pressure sensor is not read 20 times a second because the control tick asked.
    fn snapshot(&mut self) -> clevercoffee_app::Sensors;
}
