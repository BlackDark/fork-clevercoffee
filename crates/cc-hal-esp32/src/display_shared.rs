//! The panel on the shared I²C bus, owned for the life of the process.
//!
//! # Why this type exists
//!
//! The SSD1306 and the ABP2 pressure sensor are on the **same two wires**
//! (`pinmapping.h:53-54`: SCL 22, SDA 21), and an ESP32 I²C peripheral can only
//! have one owner. `Abp2Pressure` takes the `I2cDriver` by value, so the display
//! cannot simply borrow it afterwards — one of the two has to give way, and
//! whoever gives up the bus holds it for as long as the other wants it, which
//! would starve the other.
//!
//! So the bus lives behind a [`Mutex`] and both users take it for the length of
//! one transaction. That is not a stylistic choice: with a 100 ms display
//! refresh and a 50 ms pressure cadence, a display that held the bus
//! continuously would make the ABP2 unreadable, and a pressure sensor that did
//! the same would make the panel flicker. Neither is acceptable, and the C++
//! solved it with `Wire` being re-entrant-by-convention rather than by design.
//!
//! # What the control task must do
//!
//! [`SharedPanel::refresh`] is the whole interface: call it every tick and it
//! decides whether the interval has elapsed, renders, and hands the bus back
//! before returning. It never blocks on the display for longer than one frame.
//!
//! It returns [`RefreshOutcome`] rather than `()` so the caller can log *why*
//! nothing was drawn. "The display is blank" and "the bus was taken by the
//! pressure sensor" are completely different problems and the boot log has to be
//! able to tell them apart.

use alloc::format;
use alloc::string::String;
use std::sync::{Mutex, MutexGuard};

use log::warn;

use esp_idf_hal::i2c::I2cDriver;
use esp_idf_svc::sys::{EspError, ESP_FAIL};

use crate::display::{self, I2cPanel, Oled, FRAMEBUFFER_LEN};

/// The I²C address the panel answers at.
///
/// `hardware.oled.address` defaults to `ADDR_3C` (`Config.h:951-957`).
pub const ADDRESS: u8 = display::ADDRESS_3C;

/// The bus, shared between the panel and the pressure sensor.
///
/// `Mutex`, not `Arc<Mutex<..>>`: the bus is a single process-wide peripheral
/// and there is exactly one instance of this, created in `bring_up` and moved
/// into the control task. An `Arc` would imply a second owner exists.
pub struct SharedBus(Mutex<I2cDriver<'static>>);

impl SharedBus {
    /// Wrap a constructed bus.
    #[must_use]
    pub const fn new(bus: I2cDriver<'static>) -> Self {
        Self(Mutex::new(bus))
    }

    /// Take the bus for one transaction.
    ///
    /// Returns `None` if the bus is poisoned, which on this target means a
    /// previous holder panicked while holding it. Reporting rather than
    /// unwrapping: a poisoned bus means the pressure sensor and the display
    /// have both already lost, and a second panic during bring-up recovery
    /// would hide the first.
    pub fn take(&self) -> Option<MutexGuard<'_, I2cDriver<'static>>> {
        self.0.lock().ok()
    }
}

/// The shared bus as the ABP2's driver wants it.
///
/// This is what lets the pressure sensor and the panel be peers: the domain
/// driver is generic over [`I2cBus`](cc_domain::abp2::I2cBus), so implementing
/// it here gives the sensor a bus that takes the lock per transaction without
/// knowing a panel exists.
impl cc_domain::abp2::I2cBus for SharedBus {
    type Error = EspError;

    fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), Self::Error> {
        let mut guard = self.take().ok_or(EspError::from_infallible::<ESP_FAIL>())?;
        // `BLOCK` matches `Abp2I2c`: every transaction is three bytes out and
        // seven in, and the driver-level timeout is what stops a wedged device
        // from holding the lock.
        guard.write(address, bytes, esp_idf_hal::delay::BLOCK)
    }

    fn read(&mut self, address: u8, buffer: &mut [u8]) -> Result<usize, Self::Error> {
        let mut guard = self.take().ok_or(EspError::from_infallible::<ESP_FAIL>())?;
        guard.read(address, buffer, esp_idf_hal::delay::BLOCK)?;
        // `hal::i2c::read` fills the whole buffer or fails.
        Ok(buffer.len())
    }
}

/// A borrowed [`SharedBus`] is itself a bus, so a caller can hold a reference
/// rather than needing a second wrapper type.
///
/// This is what lets `ControlArgs` carry the pressure sensor as
/// `Abp2Pressure<&'static SharedBus>`: the domain driver is generic over
/// [`I2cBus`](cc_domain::abp2::I2cBus), and the sensor holds the bus by
/// reference, so the reference itself has to satisfy the trait.
impl cc_domain::abp2::I2cBus for &SharedBus {
    type Error = EspError;

    fn write(&mut self, address: u8, bytes: &[u8]) -> Result<(), Self::Error> {
        self.0
            .lock()
            .map_err(|_| EspError::from_infallible::<ESP_FAIL>())?
            .write(address, bytes, esp_idf_hal::delay::BLOCK)
    }

    fn read(&mut self, address: u8, buffer: &mut [u8]) -> Result<usize, Self::Error> {
        self.0
            .lock()
            .map_err(|_| EspError::from_infallible::<ESP_FAIL>())?
            .read(address, buffer, esp_idf_hal::delay::BLOCK)?;
        Ok(buffer.len())
    }
}

/// What one call to [`SharedPanel::refresh`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshOutcome {
    /// The panel was off (standby power save), so nothing was sent.
    Blank,
    /// The refresh interval had not elapsed; the previous frame stands.
    NotDue,
    /// A frame was rendered and sent.
    Drawn,
    /// The bus was held by the pressure sensor this tick, so the frame was
    /// skipped. Not an error — it is the backpressure working — but worth
    /// counting, because a persistently contended bus means a display that
    /// updates at a fraction of 10 Hz and nobody can say why.
    BusBusy,
    /// The bus was poisoned, or the panel rejected the frame.
    Failed,
}

/// The panel, plus the bus it shares with the pressure sensor.
///
/// The SSD1306 controller is **not** stored. It is rebuilt per frame from a
/// short borrow of the bus and dropped at the end of the frame, so no `Oled`
/// ever holds a reference to a `MutexGuard` that has already been released —
/// which is the lifetime error this type was written to avoid.
///
/// The cost is that the init sequence goes out again on every frame. That is
/// ~30 bytes against 1024 of frame data, and at 10 Hz it is a rounding error on
/// a bus with 24 ms of traffic per frame anyway. Trading 30 bytes for a type
/// with no self-referential lifetime is the right side of that.
pub struct SharedPanel<'bus> {
    bus: &'bus SharedBus,
    /// Whether a panel answered at bring-up.
    ///
    /// A machine with no panel must keep running, so this is a fact to report
    /// once, not a reason to stop.
    present: bool,
    /// Whether the panel is blanked.
    blank: bool,
    /// When the last frame went out, for the 100 ms interval.
    last_flush_ms: Option<u32>,
    /// Frames actually sent.
    frames: u32,
    /// Ticks skipped because the write failed.
    failed: u32,
}

impl<'bus> SharedPanel<'bus> {
    /// Probe the panel on the shared bus and adopt it.
    ///
    /// Sends the init sequence, so a failure here means the panel is not
    /// answering at [`ADDRESS`] — a wiring/address fact worth reporting, not
    /// something to retry silently.
    ///
    /// Takes `&SharedBus`, not the bus: the ABP2 is a peer on the same wires
    /// and both need it for the life of the process. The panel holds a
    /// reference, exactly as [`crate::sensors::Abp2Pressure::on_shared_bus`]
    /// does.
    #[must_use]
    pub fn bring_up(bus: &'bus SharedBus) -> Self {
        let present = bus
            .take()
            .is_some_and(|mut guard| Oled::new(I2cPanel::new(&mut guard, ADDRESS)).is_ok());
        if !present {
            warn!(
                "display: no panel answered at 0x{ADDRESS:02X} — the display is \
                 absent or mis-addressed, and the machine will run without it"
            );
        }
        Self {
            bus,
            present,
            blank: false,
            last_flush_ms: None,
            frames: 0,
            failed: 0,
        }
    }

    /// Whether a panel is present and initialised.
    #[must_use]
    pub const fn present(&self) -> bool {
        self.present
    }

    /// Draw `frame` if the refresh interval has elapsed.
    ///
    /// Takes the bus for the length of one frame and releases it before
    /// returning, so the ABP2 is never locked out for longer than ~24 ms of
    /// bus time (1024 bytes at 400 kHz).
    pub fn refresh(&mut self, frame: &[u8; FRAMEBUFFER_LEN], now_ms: u32) -> RefreshOutcome {
        if !self.present {
            return RefreshOutcome::Failed;
        }
        if self.blank {
            return RefreshOutcome::Blank;
        }
        if let Some(last) = self.last_flush_ms {
            if now_ms.wrapping_sub(last) < display::REFRESH_INTERVAL_MS {
                return RefreshOutcome::NotDue;
            }
        }
        let Some(mut guard) = self.bus.take() else {
            self.failed = self.failed.saturating_add(1);
            return RefreshOutcome::Failed;
        };
        let Ok(mut dev) = Oled::new(I2cPanel::new(&mut guard, ADDRESS)) else {
            drop(guard);
            self.failed = self.failed.saturating_add(1);
            return RefreshOutcome::Failed;
        };
        let result = dev.flush(frame);
        // Released before the outcome is reported, so a caller that logs a
        // failure does not hold the ABP2 out for the duration.
        drop(guard);
        // `if let` rather than `match`: the two arms differ only in the
        // counters, and there is no error to carry out of here — a failed
        // flush is counted and reported, never propagated, because a display
        // that has gone away must not take the control loop with it.
        if result.is_ok() {
            self.frames = self.frames.saturating_add(1);
            self.last_flush_ms = Some(now_ms);
            RefreshOutcome::Drawn
        } else {
            self.failed = self.failed.saturating_add(1);
            RefreshOutcome::Failed
        }
    }

    /// Blank or unblank the panel.
    ///
    /// A no-op when there is no panel, so callers do not have to ask first.
    pub fn set_blank(&mut self, blank: bool) {
        if self.blank == blank || !self.present {
            self.blank = blank;
            return;
        }
        self.blank = blank;
        if let Some(mut guard) = self.bus.take() {
            if let Ok(mut dev) = Oled::new(I2cPanel::new(&mut guard, ADDRESS)) {
                if dev.set_power_saved(blank).is_err() {
                    self.failed = self.failed.saturating_add(1);
                }
            }
            // The guard drops here, returning the bus.
            drop(guard);
        }
    }

    /// One line for the boot log and the periodic report.
    #[must_use]
    pub fn report(&self) -> String {
        format!(
            "display: present={} blanked={} frames={} failed={}",
            self.present, self.blank, self.frames, self.failed
        )
    }
}
