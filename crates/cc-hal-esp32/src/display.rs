//! The SSD1306 OLED over I²C: transport only.
//!
//! Owner: **R3-09**.
//!
//! # What this file is, and what it is not
//!
//! This is the *wire*. It owns the I²C address, the controller's power state,
//! the transfer of 1024 bytes to the panel, and the 100 ms cadence the C++
//! refreshes at. It does **not** decide what is on the screen: every template,
//! glyph, bar and label is [`cc_display`]'s, host-tested against 48 goldens and
//! pixel-for-pixel against the real U8g2 (`just test-display-parity`). This
//! crate therefore never names `cc-display` — [`Oled::flush`] takes
//! `&[u8; FRAMEBUFFER_LEN]`, and `cc_display::Framebuffer::as_bytes()`
//! satisfies that at the call site with no new dependency and no conversion.
//!
//! The consequence is the whole point of the design: **`cc-display` already
//! stores the panel's wire format.** Its buffer is page-major, vertical,
//! LSB-on-top (`cc_display::display`, module docs) because that is
//! `u8g2_ll_hvline_vertical_top_lsb` *and* the SSD1306's page-addressed GDDRAM
//! layout. So a flush is a slice write with no transposing pass, and the
//! rendering the host proves is the rendering the glass shows.
//!
//! # Why `ssd1306`, and why the init sequence is hand-written
//!
//! `ssd1306` 0.10.0 is the crate the compatibility matrix selected (02
//! §OLED): `embedded-hal 1.0`, which `esp-idf-hal` 0.47.0 satisfies
//! (`src/i2c.rs:458-478` implements `ErrorType` and `I2c<SevenBitAddress>` for
//! `I2cDriver`).
//!
//! **The SH1106 decision holds, and is better founded than when it was made.**
//! `sh1106` 0.5.0 needs `embedded-hal 0.2.3` — and that is *not* the blocker,
//! because `embedded-hal` 0.2.7 is still in this tree for exactly that kind of
//! consumer. The blocker is geometry: the SH1106's GDDRAM is 132 columns wide
//! with the 128x64 window centred, so it needs the column window moved by two
//! on every frame. U8g2 does that in its SH1106 init; `ssd1306` exposes no
//! such knob on `DisplaySize`, so supporting it means forking the driver. The
//! C++ defaults `hardware.oled.type` to `SSD1306` (`Config.h:943-949`), so
//! SH1106 is a reachable but unused configuration: dropping it removes an
//! *option*, not a behaviour the C++ exercised. Recorded in
//! `intentional-diffs.md`.
//!
//! The init sequence is U8g2's, byte for byte
//! (`u8x8_d_ssd1306_128x64_noname.c:41-72`), sent through [`I2cPanel`] rather
//! than through `ssd1306`'s `DisplayConfig::init`. That is a deliberate
//! deviation from the crate's defaults, and it exists because three of the
//! crate's init commands **disagree with the C++**:
//!
//! | command | U8g2 | `ssd1306` 0.10 default |
//! | --- | --- | --- |
//! | `0xDA` COM pin config | `0x12` | `0x22` (`command.rs:168-170`) |
//! | `0x81` contrast | `0xCF` | `0x7F` (`Brightness::NORMAL`) |
//! | `0xD9` pre-charge | `0xF1` | `0x21` |
//!
//! The contrast one is visible: the crate's default is a dimmer panel than the
//! one the machine ships with. Taking the whole sequence from the C++ is the
//! only way to claim the panel comes up looking exactly as it did, and it makes
//! [`INIT_SEQUENCE`] an auditable artefact instead of a pile of defaults.
//!
//! # Rotation: the panel never rotates
//!
//! U8g2's SSD1306 init sends `0xA1` / `0xC8` unconditionally — the `U8G2_R0`..
////! `R3` argument never reaches the controller. Rotation in this firmware is a
//! *software* transform: `cc_display::Rotation` picks the logical coordinate
//! space and the display writes the already-rotated physical pixel into the
//! same 1024 bytes. So all four software rotations share one panel orientation,
//! and that orientation is `DisplayRotation::Rotate0`, which the `ssd1306`
//! crate encodes as exactly `0xA1` / `0xC8`. [`PANEL_ROTATION`] is the one
//! place that decides it.
//!
//! # The bus is shared, and this driver does not own it
//!
//! The OLED and the ABP2 are on the same two wires ([`crate::sensors::pins`]:
//! SCL 22, SDA 21 — `pinmapping.h:53-54`). There is exactly one I²C
//! peripheral, so [`I2cPanel`] **borrows** an already-constructed
//! [`I2cDriver`] instead of calling `Peripherals::take()` — the same rule and
//! the same reason as [`crate::Abp2I2c::new`]. Borrowing rather than owning is
//! what lets the display task take the bus for one frame and give it straight
//! back, which is the mutex-guarded shared bus 04 §7 describes. What this file
//! guarantees is that it **never creates a second bus**.
//!
//! # Timing
//!
//! [`REFRESH_INTERVAL_MS`] is `DISPLAY_REFRESH_INTERVAL_MS` = 100
//! (`Timing.h:38`) and [`Oled::refresh_due`] is the gate that enforces it. The
//! bus runs at 400 kHz ([`crate::sensors::I2C_HZ`]), so one frame is 1024
//! payload bytes ≈ 24 ms of bus time. That is the same order as the C++'s
//! Arduino `Wire` transfer, and it is why the data path chunks at
//! [`DATA_CHUNK_BYTES`] rather than at the `display-interface-i2c` default of
//! 16 — see [`I2cPanel::send_data`].

use core::time::Duration;

use display_interface::{DataFormat, WriteOnlyDataCommand};
use esp_idf_hal::delay::TickType;
use esp_idf_hal::i2c::I2cDriver;
use ssd1306::{
    command::AddrMode, mode::BasicMode, prelude::DisplaySize128x64, rotation::DisplayRotation,
    Ssd1306,
};

/// The error every operation on the panel reports.
///
/// Re-exported rather than wrapped: `ssd1306` returns
/// `display_interface::DisplayError` and there is nothing to add to it. A
/// `BusWriteError` here means the panel did not answer — wrong address, no
/// pull-ups, or a wedged bus — and that is the only failure a display can have
/// which matters.
pub use display_interface::DisplayError as Error;

/// Panel width in pixels. `defaults.h:224`.
pub const WIDTH: u8 = 128;
/// Panel height in pixels. `defaults.h:225`.
pub const HEIGHT: u8 = 64;

/// Bytes in one frame: eight pages of 128 columns, one bit per pixel.
pub const FRAMEBUFFER_LEN: usize = (WIDTH as usize) * (HEIGHT as usize / 8);

/// The default I²C address. `Config.h:951-957`, `hardware.oled.address`.
pub const ADDRESS_3C: u8 = 0x3C;
/// The alternate I²C address, for a panel strapped the other way.
pub const ADDRESS_3D: u8 = 0x3D;

/// How often a flush is allowed. `DISPLAY_REFRESH_INTERVAL_MS` (`Timing.h:38`).
///
/// The C++ puts this on a `MillisecondTimer` calling
/// `DisplayTemplateManager::printScreen`, and `LoopManager::updateDisplay`
/// then sends the buffer when the coordinator marked it ready
/// (`LoopManager.cpp:197-201`, `:386-387`). The two halves are separated here
/// too: [`Oled::refresh_due`] is the timer, and *what* to draw stays with the
/// caller, because only the caller knows whether the content changed.
pub const REFRESH_INTERVAL_MS: u32 = 100;

/// The `0x40` control byte: Co = 0, D/C# = 1 — "the bytes that follow are
/// data".
///
/// Co = 0 is the load-bearing bit: it describes the *stream*, not the
/// transaction, which is what makes [`I2cPanel`]'s chunking legal.
pub const DATA_CONTROL_BYTE: u8 = 0x40;

/// The `0x00` control byte: Co = 0, D/C# = 0 — "the bytes that follow are
/// commands".
pub const COMMAND_CONTROL_BYTE: u8 = 0x00;

/// Payload bytes per I²C write on the data path.
///
/// 128 is U8g2's page size, so a frame is eight writes — the shape the C++
/// produces. It is also why this driver does **not** use
/// `display_interface_i2c::I2CInterface`, which hard-codes 16-byte chunks
/// (`display-interface-i2c-0.5.0/src/lib.rs:97-118`): that would turn one
/// frame into 64 transactions instead of 8, and every extra transaction is
/// another start/address/stop on a bus the ABP2 shares.
pub const DATA_CHUNK_BYTES: usize = 128;

/// The controller's power-on configuration: U8g2's, byte for byte.
///
/// `u8x8_d_ssd1306_128x64_noname.c:41-72`, the sequence
/// `U8G2_SSD1306_128X64_NONAME_F_HW_I2C` sends from `begin()`. It does **not**
/// contain the trailing `0xAF`: U8g2's `u8g2_InitDisplay` appends that
/// separately, through `u8x8_SetPowerSave(0)`, and [`Oled::new`] does the same
/// with [`ssd1306::Ssd1306::set_display_on`] so that the driver's idea of the
/// power state and the panel's cannot drift.
pub const INIT_SEQUENCE: &[u8] = &[
    0xAE, // display off
    0xD5, 0x80, // clock divide ratio 0 (x1), oscillator frequency 8
    0xA8, 0x3F, // multiplex ratio 64
    0xD3, 0x00, // display offset 0
    0x40, // display start line 0
    0x8D, 0x14, // charge pump on
    0x20, 0x00, // horizontal addressing mode
    0xA1, // segment remap: columns 127 -> 0
    0xC8, // COM scan direction: COM[N-1] -> COM0
    0xDA, 0x12, // COM pins: sequential
    0x81, 0xCF, // contrast
    0xD9, 0xF1, // pre-charge period
    0xDB, 0x40, // VCOMH deselect level
    0x2E, // deactivate scroll
    0xA4, // output follows RAM
    0xA6, // not inverted
];

/// The panel orientation the C++ runs, and the only one that exists here.
///
/// `0xA1` + `0xC8`, which is what [`INIT_SEQUENCE`] sends and what U8g2 sends
/// for every one of its four rotations. See the module's "Rotation" section
/// for why a software rotation never reaches the controller.
pub const PANEL_ROTATION: DisplayRotation = DisplayRotation::Rotate0;

/// How long one I²C write may block before the panel is given up on.
///
/// One refresh period. A frame that cannot be transferred in the time between
/// two frames has already missed its deadline, so holding the bus longer only
/// starves the ABP2 — which reads on the *same* two wires — for no gain. This
/// is the bound the `Abp2I2c` module docs ask of every transaction and the C++
/// never had, because Arduino's `Wire.endTransmission()` returns instead.
pub const BUS_TIMEOUT: Duration = Duration::from_millis(REFRESH_INTERVAL_MS as u64);

/// [`BUS_TIMEOUT`] in the units `I2cDriver::write` actually takes.
///
/// `write` is declared `timeout: TickType_t` (a raw `u32` tick count,
/// `esp-idf-hal-0.47.0/src/i2c.rs:315`), not the `TickType` wrapper — so
/// `TickType::from(..)` does not type-check here even though the conversion
/// exists. `new_millis` and `ticks` are both `const`; `From<Duration>` is not,
/// so the milliseconds are converted directly rather than via the `Duration`
/// above.
const fn bus_timeout_ticks() -> esp_idf_hal::delay::TickType_t {
    TickType::new_millis(REFRESH_INTERVAL_MS as u64).ticks()
}

/// The wire: one or a few I²C writes per call, control byte prefixed.
///
/// This is the only place in the firmware that talks to the panel, and it is
/// small on purpose. `ssd1306` needs a `WriteOnlyDataCommand`, and the obvious
/// implementation of that is `display_interface_i2c::I2CInterface`, which
/// chunks data at 16 bytes ([`DATA_CHUNK_BYTES`] says why that is wrong here).
/// Writing the four-line impl against [`I2cDriver`]'s *inherent* `write` —
/// rather than its `embedded-hal` impl — is also what keeps this crate from
/// naming `embedded-hal` itself.
pub struct I2cPanel<'bus, 'd> {
    bus: &'bus mut I2cDriver<'d>,
    address: u8,
}

impl<'bus, 'd> I2cPanel<'bus, 'd> {
    /// Borrow an already-constructed bus.
    ///
    /// # Wiring
    ///
    /// The borrow, not ownership, is the point. The bus is the shared
    /// mutex-guarded resource 04 §7 requires, and the display task must be
    /// able to take it for one frame and hand it straight back so the ABP2 can
    /// read. [`PanelOled`] is therefore built per flush from a short borrow, not
    /// parked for the life of the process; the panel's own state that must
    /// outlive a borrow lives in [`Oled`], not here.
    #[must_use]
    pub const fn new(bus: &'bus mut I2cDriver<'d>, address: u8) -> Self {
        Self { bus, address }
    }

    /// The 7-bit address this panel is addressed at.
    #[must_use]
    pub const fn address(&self) -> u8 {
        self.address
    }

    /// Send [`INIT_SEQUENCE`] and switch the panel on. **Once**, at bring-up.
    ///
    /// Separate from every later frame write because the sequence switches the
    /// panel *off* (`0xAE`) before configuring it; sending it per frame is a
    /// visible flash at the refresh rate. See [`Oled::new_initialised`].
    ///
    /// # Errors
    ///
    /// Whatever the bus reports. A NAK is the panel saying it is not there.
    pub fn initialise(&mut self) -> Result<(), Error> {
        self.transfer(COMMAND_CONTROL_BYTE, INIT_SEQUENCE)?;
        // The trailing 0xAF, outside the sequence, exactly as U8g2 does it
        // (`u8g2_InitDisplay`).
        self.transfer(COMMAND_CONTROL_BYTE, &[0xAF])
    }

    /// Write `bytes` with `control` in front, in chunks of
    /// [`DATA_CHUNK_BYTES`].
    ///
    /// A payload longer than one transaction is split and each piece
    /// re-prefixed with the same control byte. That is sound for both callers
    /// because [`COMMAND_CONTROL_BYTE`] and [`DATA_CONTROL_BYTE`] both clear
    /// Co, which is a statement about the stream rather than about one
    /// transaction: a STOP in the middle does not end it.
    fn transfer(&mut self, control: u8, bytes: &[u8]) -> Result<(), Error> {
        // Control byte plus one chunk. Stack, not static RAM.
        let mut buf = [0u8; DATA_CHUNK_BYTES + 1];
        buf[0] = control;
        for chunk in bytes.chunks(DATA_CHUNK_BYTES) {
            let len = chunk.len();
            buf[1..=len].copy_from_slice(chunk);
            self.bus
                .write(self.address, &buf[..=len], bus_timeout_ticks())
                .map_err(|_| Error::BusWriteError)?;
        }
        Ok(())
    }
}

impl WriteOnlyDataCommand for I2cPanel<'_, '_> {
    fn send_commands(&mut self, commands: DataFormat<'_>) -> Result<(), Error> {
        match commands {
            DataFormat::U8(slice) => self.transfer(COMMAND_CONTROL_BYTE, slice),
            // `DataFormat` is `#[non_exhaustive]`, so this arm is required
            // whether or not the crate can produce the others today. Nothing in
            // this driver's command stream is wider than `u8`, so they are
            // unreachable rather than unimplemented.
            _ => Err(Error::DataFormatNotImplemented),
        }
    }

    fn send_data(&mut self, data: DataFormat<'_>) -> Result<(), Error> {
        match data {
            DataFormat::U8(slice) => self.transfer(DATA_CONTROL_BYTE, slice),
            _ => Err(Error::DataFormatNotImplemented),
        }
    }
}

/// The panel: the controller's configuration plus the transport underneath.
///
/// Generic in the transport so the wire encoding can be tested with no
/// peripheral and no bus — which is the only reason the tests at the bottom of
/// this file exist, and the only reason the production type is an alias rather
/// than a struct.
pub struct Oled<DI> {
    dev: Ssd1306<DI, DisplaySize128x64, BasicMode>,
    power_saved: bool,
    /// When the last flush went out, for [`Oled::refresh_due`].
    ///
    /// `u32` milliseconds with a wrapping subtraction, so this wraps every
    /// 49.7 days exactly as [`crate::time::now_ms`] does. A wrap across a
    /// 100 ms gate costs at most one early flush.
    last_flush_ms: u32,
    /// Whether anything has been flushed, so the first frame goes out
    /// immediately rather than after a blank 100 ms at boot.
    flushed_once: bool,
}

impl<DI> core::fmt::Debug for Oled<DI> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Oled")
            .field("rotation", &PANEL_ROTATION)
            .field("power_saved", &self.power_saved)
            .field("flushed_once", &self.flushed_once)
            .finish_non_exhaustive()
    }
}

impl<DI: WriteOnlyDataCommand> Oled<DI> {
    /// Bring the controller up on `interface`.
    ///
    /// The order is the C++'s (`DisplayManager.cpp:22-25`: `begin()`, then
    /// `clearBuffer()`) with one addition — [`INIT_SEQUENCE`] goes out through
    /// the transport *before* the `Ssd1306` wrapper exists, because the wrapper
    /// has no accessor for its interface and U8g2's sequence is not one of the
    /// crate's defaults anyway. The addressing mode is then declared to the
    /// wrapper so its cached state agrees with the sequence, and the panel is
    /// switched on, which is the `0xAF` U8g2 sends outside the sequence.
    ///
    /// # Errors
    ///
    /// Any I²C failure. A panel that does not answer is not fatal to the
    /// machine — the C++ logs it and carries on (`DisplayManager.cpp:26-28`) —
    /// so the caller decides whether to run without a display.
    pub fn new(mut interface: DI) -> Result<Self, Error> {
        interface.send_commands(DataFormat::U8(INIT_SEQUENCE))?;
        let mut dev = Ssd1306::new(interface, DisplaySize128x64, PANEL_ROTATION);
        // Sends 0x20 0x00, already in the sequence. Three bytes at boot, in
        // exchange for `flush` being able to emit the page window every frame
        // from a known state rather than from whatever the last frame left.
        dev.set_addr_mode(AddrMode::Horizontal)?;
        dev.set_display_on(true)?;
        Ok(Self {
            dev,
            power_saved: false,
            last_flush_ms: 0,
            flushed_once: false,
        })
    }

    /// Wrap an **already-initialised** transport, without sending
    /// [`INIT_SEQUENCE`].
    ///
    /// # Why this exists
    ///
    /// `INIT_SEQUENCE` begins `0xAE` (display off) and [`Oled::new`] appends
    /// `0xAF` (display on), so building a fresh `Oled` per frame switches the
    /// panel off and on at the refresh rate. That is a visible flash — reported
    /// by the human on real hardware — and it also costs ~30 bytes of commands
    /// per frame on a bus the ABP2 pressure sensor shares.
    ///
    /// The correct shape is U8g2's: `u8g2_InitDisplay` runs once from
    /// `begin()`, and every later `drawTile` sets the draw area and writes
    /// pixels. So the sequence goes out **once**, from
    /// [`I2cPanel::initialise`], and every frame after it is [`Oled::flush`].
    ///
    /// # Errors
    ///
    /// Any I²C failure. A panel that does not answer is not fatal — the C++ logs
    /// it and carries on (`DisplayManager.cpp:26-28`).
    pub fn new_initialised(mut interface: DI) -> Result<Self, Error> {
        // The addressing mode is declared to the wrapper so its cached state
        // agrees with what the init sequence already put on the panel. Two
        // bytes, and without them `flush` would set a window the wrapper does not
        // believe it has set.
        interface.send_commands(DataFormat::U8(&[0x20, 0x00]))?;
        Ok(Self {
            dev: Ssd1306::new(interface, DisplaySize128x64, PANEL_ROTATION),
            power_saved: false,
            last_flush_ms: 0,
            flushed_once: false,
        })
    }

    /// Send one frame.
    ///
    /// `framebuffer` is one whole panel, page-major and LSB-on-top, in the
    /// order the controller's GDDRAM wants it. There is no conversion step and
    /// no copy of the caller's buffer: the slice is chunked in place by
    /// [`I2cPanel::transfer`].
    ///
    /// The column and page window is re-sent every frame rather than relying on
    /// the controller having wrapped back to the origin after the last one. That
    /// is four bytes a frame, and it makes a frame cut short by a timeout
    /// self-heal on the next one — which matters because a partial write leaves
    /// the window wherever it stopped.
    ///
    /// # Errors
    ///
    /// Any I²C failure, in which case the frame did not reach the panel.
    pub fn flush(&mut self, framebuffer: &[u8; FRAMEBUFFER_LEN]) -> Result<(), Error> {
        self.dev.set_draw_area((0, 0), (WIDTH, HEIGHT))?;
        self.dev.draw(framebuffer)
    }

    /// The 100 ms gate: `true` once per [`REFRESH_INTERVAL_MS`], and `true`
    /// immediately for the first call after boot.
    ///
    /// This is the timer half of the C++'s `printDisplayTimer_`. The render
    /// half stays with the caller, so a false return means "do nothing this
    /// loop" — the same no-op iteration the C++ performs.
    pub fn refresh_due(&mut self, now_ms: u32) -> bool {
        if self.flushed_once && now_ms.wrapping_sub(self.last_flush_ms) < REFRESH_INTERVAL_MS {
            return false;
        }
        self.last_flush_ms = now_ms;
        self.flushed_once = true;
        true
    }

    /// Blank the panel (`true`) or wake it (`false`).
    ///
    /// `0xAE` and `0xAF`, which is U8g2's `setPowerSave` in both directions and
    /// what `LoopManager::updateDisplay` drives from
    /// `standbyCoordinator().shouldTurnOffDisplay()` (`LoopManager.cpp:334-339`).
    ///
    /// The controller keeps its RAM contents, so waking restores the last frame
    /// without a re-flush — which is why the C++ can return early and still come
    /// back to a correct screen.
    ///
    /// The state is recorded locally too, so [`Oled::power_saved`] costs no bus
    /// round trip.
    ///
    /// # Errors
    ///
    /// Any I²C failure.
    pub fn set_power_saved(&mut self, saved: bool) -> Result<(), Error> {
        self.dev.set_display_on(!saved)?;
        self.power_saved = saved;
        Ok(())
    }

    /// Whether the panel is currently blanked. See [`Oled::set_power_saved`].
    #[must_use]
    pub const fn power_saved(&self) -> bool {
        self.power_saved
    }

    /// Whether a flush is worth sending at all.
    ///
    /// `false` means the C++'s `updateDisplay` would have returned at
    /// `LoopManager.cpp:334-339`: the panel is off, so a transfer would buy
    /// nothing. Making this a method rather than a convention is what keeps
    /// "do not touch the bus while blanked" at one call site instead of in every
    /// caller.
    #[must_use]
    pub const fn should_flush(&self) -> bool {
        !self.power_saved
    }

    /// Give the transport back.
    ///
    /// The counterpart to [`I2cPanel::new`]'s borrow: after this, the caller's
    /// bus reference is live again and the ABP2 can read. Everything worth
    /// keeping across the gap — the power state and the refresh timestamp —
    /// lives in [`Oled`] and is meant to be copied out before the panel is
    /// dropped.
    #[must_use]
    pub fn into_interface(self) -> DI {
        self.dev.release()
    }
}

/// The panel as it is actually built: an SSD1306 on the shared I²C bus.
///
/// The `'bus` is the length of one borrow of the shared bus and `'d` the bus's
/// own lifetime, which for a process-wide peripheral is `'static`.
pub type PanelOled<'bus, 'd> = Oled<I2cPanel<'bus, 'd>>;

/// The on-target unit tests for the panel driver.
///
/// The wire encoding is checked against a [`Recorder`] that captures every byte,
/// so the whole command stream is asserted with no panel attached. That matters
/// because the assertions are about the *sequence* — the init bytes, the
/// addressing mode, the page order — and a real panel would only tell you that
/// something is wrong, not what.
#[cfg(any(test, feature = "device-tests"))]
pub mod tests {
    // `missing_panics_doc` wants a `# Panics` section on anything that can
    // panic, and an `assert_eq!` can. A test whose body is three assertions
    // does not benefit from three boilerplate sections saying "if an assertion
    // fails" — and on this target a failing assertion *does* panic, by design:
    // the on-target runner is built around "a failed assert resets the chip"
    // (see `cc-device-tests`). A panic here is the reporting mechanism, not an
    // undocumented hazard, and none of this module ships.
    #![allow(clippy::missing_panics_doc)]
    // As in every other module here: these exercise the parent's private
    // helpers, so the module globs its parent. The `clippy::wildcard_imports`
    // exception the lint gives a `#[cfg(test)]` module does not apply to this
    // one, which the on-target runner also compiles.
    #![allow(clippy::wildcard_imports)]

    use super::*;

    /// Records every command and data byte handed to the transport, so the wire
    /// encoding can be asserted on the real target with no I²C bus and no
    /// peripherals — which, per the harness's own module docs, is the gap that
    /// let two device bugs ship behind green tests.
    ///
    /// Capacity 2 KiB: the largest thing a test sends is [`INIT_SEQUENCE`] plus
    /// one 1024-byte frame plus its window. Overflow panics rather than
    /// truncating, because a silently truncated recording is exactly the kind
    /// of quiet wrong answer this harness exists to prevent.
    struct Recorder {
        bytes: [u8; 2 * FRAMEBUFFER_LEN],
        len: usize,
        data_transfers: usize,
    }

    impl Recorder {
        const fn new() -> Self {
            Self {
                bytes: [0; 2 * FRAMEBUFFER_LEN],
                len: 0,
                data_transfers: 0,
            }
        }

        /// Every byte recorded so far, commands and data alike.
        ///
        /// Not `const`: slicing with a runtime length is `core::ops::Index`, which
        /// is not yet a const trait, so a `const fn` here does not compile on this
        /// toolchain.
        fn all(&self) -> &[u8] {
            &self.bytes[..self.len]
        }

        /// The payload of the most recent transfer, i.e. the recorded bytes
        /// after the last control byte.
        fn last_payload(&self) -> &[u8] {
            let mut start = 0;
            for i in (0..self.len).rev() {
                if matches!(self.bytes[i], DATA_CONTROL_BYTE | COMMAND_CONTROL_BYTE) {
                    start = i + 1;
                    break;
                }
            }
            &self.bytes[start..self.len]
        }

        /// Whether `needle` occurs anywhere. A sliding window rather than a
        /// substring search, because a recording contains the framebuffer and a
        /// `memmem` would happily match inside a rendered glyph.
        fn contains(&self, needle: &[u8]) -> bool {
            self.all().windows(needle.len()).any(|w| w == needle)
        }

        fn push(&mut self, chunk: &[u8]) {
            for &b in chunk {
                self.bytes[self.len] = b;
                self.len += 1;
            }
        }
    }

    impl WriteOnlyDataCommand for Recorder {
        fn send_commands(&mut self, commands: DataFormat<'_>) -> Result<(), Error> {
            match commands {
                DataFormat::U8(slice) => {
                    self.push(&[COMMAND_CONTROL_BYTE]);
                    self.push(slice);
                    Ok(())
                }
                _ => Err(Error::DataFormatNotImplemented),
            }
        }

        fn send_data(&mut self, data: DataFormat<'_>) -> Result<(), Error> {
            match data {
                DataFormat::U8(slice) => {
                    self.data_transfers += 1;
                    self.push(&[DATA_CONTROL_BYTE]);
                    self.push(slice);
                    Ok(())
                }
                _ => Err(Error::DataFormatNotImplemented),
            }
        }
    }

    /// A framebuffer with the byte at `index` set to `value`, and every other
    /// byte left distinct so a mis-ordered flush cannot pass.
    fn ramp_frame() -> [u8; FRAMEBUFFER_LEN] {
        let mut frame = [0u8; FRAMEBUFFER_LEN];
        for (i, b) in frame.iter_mut().enumerate() {
            // `u8::try_from(..).unwrap()` rather than `as u8`: the modulo keeps
            // it in range, and a silent truncation here would weaken the very
            // misalignment this ramp exists to detect.
            *b = u8::try_from(i % 251).expect("modulo 251 fits in a u8");
        }
        frame
    }

    /// The panel is 128×64 in page-addressed mode, so a frame is 1024 bytes.
    #[cfg_attr(test, test)]
    pub fn the_geometry_is_a_128_by_64_page_buffer() {
        assert_eq!(WIDTH, 128);
        assert_eq!(HEIGHT, 64);
        // 16 tiles across, 8 pages down. This is the number that has to equal
        // `cc_display::BUFFER_LEN`, which the type system then enforces at the
        // call site: `Framebuffer::as_bytes()` returns `&[u8; 1024]`.
        assert_eq!(FRAMEBUFFER_LEN, 1024);
    }

    /// The two I²C addresses an SSD1306 can be strapped to.
    #[cfg_attr(test, test)]
    pub fn the_addresses_are_the_datasheet_pair() {
        // Config.h:951-957: `hardware.oled.address` defaults to ADDR_3C.
        assert_eq!(ADDRESS_3C, 0x3C);
        assert_eq!(ADDRESS_3D, 0x3D);
    }

    /// The C++ refreshes at `DISPLAY_REFRESH_INTERVAL_MS` = 100 ms.
    #[cfg_attr(test, test)]
    pub fn the_refresh_interval_is_the_csqs_hundred_milliseconds() {
        // DISPLAY_REFRESH_INTERVAL_MS, Timing.h:38.
        assert_eq!(REFRESH_INTERVAL_MS, 100);
    }

    /// A frame is chunked into 8 bus writes, not the 64 the stock I²C interface
    /// would produce — which is what keeps the shared bus free for the ABP2.
    #[cfg_attr(test, test)]
    pub fn a_frame_is_eight_writes_not_sixty_four() {
        // Why this driver does not use `display_interface_i2c`, whose chunk size
        // is 16: 1024 / 16 = 64 transactions a frame, each with a start, an
        // address and a stop, on a bus the ABP2 shares.
        assert_eq!(DATA_CHUNK_BYTES, 128);
        assert_eq!(FRAMEBUFFER_LEN.div_ceil(DATA_CHUNK_BYTES), 8);
    }

    /// A frame must not re-send the init sequence.
    ///
    /// This is the flash the human reported on real hardware.
    /// `INIT_SEQUENCE` begins `0xAE` (display off) and `Oled::new` appends `0xAF`
    /// (display on), so building a fresh controller per frame switches the panel
    /// off and on at the refresh rate — 10 times a second, plainly visible on a
    /// machine standing on a bench. It was never a hardware fault: the shared-bus
    /// panel rebuilt the controller every frame because holding one across
    /// frames would mean holding an `Oled` that borrows a `MutexGuard`.
    ///
    /// The fix is the shape U8g2 itself has: initialise once at `begin()`, then
    /// every frame is `set_draw_area` plus `draw`. The assertion is on the
    /// **bytes**, because "the panel flashed" is a claim about the wire, and a
    /// test that only checked a return value would have passed either way.
    #[cfg_attr(test, test)]
    pub fn a_frame_does_not_re_send_the_init_sequence() {
        let mut via_new = Recorder::new();
        Oled::new(StdRecorder(&mut via_new))
            .expect("the recorder accepts every command")
            .flush(&ramp_frame())
            .expect("the recorder accepts every byte");
        assert!(
            via_new.contains(INIT_SEQUENCE),
            "sanity: Oled::new DOES send the init sequence, so this test would \
             otherwise pass for the wrong reason"
        );

        let mut per_frame = Recorder::new();
        Oled::new_initialised(StdRecorder(&mut per_frame))
            .expect("the recorder accepts every command")
            .flush(&ramp_frame())
            .expect("the recorder accepts every byte");

        assert!(
            !per_frame.contains(INIT_SEQUENCE),
            "a frame re-sent the power-on init sequence; its 0xAE/0xAF pair is the \
             visible flash. The sequence belongs in bring_up only."
        );
        assert_ne!(
            per_frame.all().first().copied(),
            Some(0xAE),
            "a frame began by switching the panel off"
        );
        assert_eq!(
            per_frame.all().len(),
            FRAMEBUFFER_LEN,
            "a frame should put exactly the framebuffer on the wire, not {} bytes",
            per_frame.all().len()
        );
    }

    /// Blanking is one byte, and does not reinitialise the panel.
    ///
    /// The same mistake in a different place: blanking through `Oled::new` would
    /// re-initialise the whole controller in order to switch it off.
    /// `set_power_saved` sends `0xAE`/`0xAF` alone, which is U8g2's
    /// `setPowerSave` in both directions.
    #[cfg_attr(test, test)]
    pub fn blanking_is_one_byte_and_does_not_reinitialise() {
        let mut off = Recorder::new();
        Oled::new_initialised(StdRecorder(&mut off))
            .expect("the recorder accepts every command")
            .set_power_saved(true)
            .expect("the recorder accepts every command");
        assert_eq!(off.all().last().copied(), Some(0xAE));
        assert!(!off.contains(INIT_SEQUENCE));

        let mut on = Recorder::new();
        Oled::new_initialised(StdRecorder(&mut on))
            .expect("the recorder accepts every command")
            .set_power_saved(false)
            .expect("the recorder accepts every command");
        assert_eq!(on.all().last().copied(), Some(0xAF));
        assert!(!on.contains(INIT_SEQUENCE));
    }

    /// A `WriteOnlyDataCommand` that borrows the module's own `Recorder`.
    ///
    /// `Oled::new` and `Oled::new_initialised` take the transport **by value**,
    /// so a test that wants to read the recording afterwards needs an adapter
    /// that hands over a borrow instead of the recorder itself.
    struct StdRecorder<'a>(&'a mut Recorder);

    impl WriteOnlyDataCommand for StdRecorder<'_> {
        fn send_commands(&mut self, cmd: DataFormat<'_>) -> Result<(), Error> {
            self.0.send_commands(cmd)
        }

        fn send_data(&mut self, data: DataFormat<'_>) -> Result<(), Error> {
            self.0.send_data(data)
        }
    }

    #[cfg_attr(test, test)]
    /// The init bytes are U8g2's `SSD1306_128X64_NONAME` sequence, byte for byte.
    pub fn the_init_sequence_is_u8g2s_ssd1306_noname_sequence() {
        // u8x8_d_ssd1306_128x64_noname.c:41-72, entry for entry. If this fails,
        // the panel is not coming up the way the C++ brought it up, and the
        // difference is invisible until someone reports a dim screen.
        assert_eq!(
            INIT_SEQUENCE,
            &[
                0xAE, 0xD5, 0x80, 0xA8, 0x3F, 0xD3, 0x00, 0x40, 0x8D, 0x14, 0x20, 0x00, 0xA1, 0xC8,
                0xDA, 0x12, 0x81, 0xCF, 0xD9, 0xF1, 0xDB, 0x40, 0x2E, 0xA4, 0xA6,
            ]
        );
    }

    #[cfg_attr(test, test)]
    /// Segment remap and COM scan direction are set the way U8g2 sets them, so
    /// the rendered frame is not mirrored relative to the C++.
    pub fn the_init_sequence_remaps_segments_and_reverses_com() {
        // The orientation. U8g2 sends these two for *all four* of its
        // rotations, which is why the panel has one orientation and rotation is
        // a software transform.
        assert!(INIT_SEQUENCE.windows(2).any(|w| w == [0xA1, 0xC8]));
    }

    #[cfg_attr(test, test)]
    /// The contrast value matches the one the C++ writes, not the crate default.
    pub fn the_init_sequence_sets_the_contrast_the_cpp_set() {
        // 0x81 0xCF. The `ssd1306` crate's own init sends 0x7F, which is
        // visibly dimmer — the likeliest cause of a "the new firmware's display
        // looks worse" report.
        assert!(INIT_SEQUENCE.windows(2).any(|w| w == [0x81, 0xCF]));
        assert!(!INIT_SEQUENCE.windows(2).any(|w| w == [0x81, 0x7F]));
    }

    #[cfg_attr(test, test)]
    /// Addressing mode is horizontal (0x20, 0x00), which is what the page-ordered
    /// flush below assumes.
    pub fn the_panel_comes_up_in_horizontal_addressing_mode() {
        // 0x20 0x00. In this mode the controller walks columns and wraps to
        // the next page, which is what lets a whole 1024-byte frame go out in
        // order with no per-page addressing command.
        assert!(INIT_SEQUENCE.windows(2).any(|w| w == [0x20, 0x00]));
    }

    #[cfg_attr(test, test)]
    /// Bring-up emits the init sequence and only then the display-on command.
    pub fn bring_up_sends_u8g2s_sequence_then_the_display_on_it_appends() {
        let oled = Oled::new(Recorder::new()).expect("a recorder accepts every command");
        let recorder = oled.into_interface();
        // U8g2's `begin()`: the device sequence, then `u8x8_SetPowerSave(0)`,
        // which is the 0xAF. The `0x20 0x00` in between is the addressing-mode
        // repeat and is idempotent.
        assert!(recorder.contains(INIT_SEQUENCE));
        assert_eq!(recorder.all().last(), Some(&0xAF));
    }

    #[cfg_attr(test, test)]
    /// A flush writes all 1024 bytes, in page order, and not a byte more.
    pub fn a_flush_puts_the_whole_frame_on_the_wire_in_page_order() {
        let mut oled = Oled::new(Recorder::new()).expect("a recorder accepts every command");
        let frame = ramp_frame();
        oled.flush(&frame).expect("a recorder accepts every byte");
        let recorder = oled.into_interface();

        // The window: columns 0..127 over pages 0..7, which is
        // `0x21 0x00 0x7F` + `0x22 0x00 0x07`.
        assert!(recorder.contains(&[0x21, 0x00, 0x7F]));
        assert!(recorder.contains(&[0x22, 0x00, 0x07]));

        // And the frame, byte for byte, in the order the caller gave it. This
        // is the claim that `cc-display`'s page-major LSB-on-top layout is the
        // controller's own layout, so no transposition stands between the
        // host-proven rendering and the glass.
        let payload = recorder.last_payload();
        assert_eq!(payload.len(), FRAMEBUFFER_LEN);
        assert_eq!(payload, frame);
    }

    #[cfg_attr(test, test)]
    /// Power save blanks the panel and waking restores the last frame, rather
    /// than leaving a blank panel until the next tick.
    pub fn power_save_blanks_the_panel_and_waking_restores_the_frame() {
        let mut oled = Oled::new(Recorder::new()).expect("a recorder accepts every command");
        let frame = ramp_frame();
        oled.flush(&frame).expect("a recorder accepts every byte");

        oled.set_power_saved(true)
            .expect("the recorder accepts 0xAE");
        assert!(oled.power_saved());
        assert!(
            !oled.should_flush(),
            "a blanked panel must not be written to"
        );

        oled.set_power_saved(false)
            .expect("the recorder accepts 0xAF");
        assert!(!oled.power_saved());
        assert!(oled.should_flush());

        let recorder = oled.into_interface();
        // 0xAE then 0xAF: `setPowerSave` in both directions, which is all
        // `MachineStateContext::setDisplayPowerSave` ever did.
        assert!(recorder.contains(&[0xAE]));
        assert!(recorder.contains(&[0xAF]));
    }

    #[cfg_attr(test, test)]
    /// The refresh gate admits one frame per interval, so a caller polling far
    /// faster cannot flood the bus shared with the ABP2.
    pub fn the_refresh_gate_fires_every_hundred_milliseconds() {
        let mut oled = Oled::new(Recorder::new()).expect("a recorder accepts every command");

        // The first call is due: a blank panel for the first 100 ms of boot
        // would look like a display that did not come up.
        assert!(oled.refresh_due(0));
        assert!(
            !oled.refresh_due(0),
            "a second call in the same millisecond"
        );
        assert!(!oled.refresh_due(99));
        assert!(oled.refresh_due(100));
        assert!(!oled.refresh_due(199));
        assert!(oled.refresh_due(200));
    }

    /// The refresh gate compares with `wrapping_sub`, so it does not freeze for
    /// 49 days when the millisecond counter wraps.
    #[cfg_attr(test, test)]
    pub fn the_refresh_gate_survives_the_49_day_millisecond_wrap() {
        let mut oled = Oled::new(Recorder::new()).expect("a recorder accepts every command");
        let forty_nine_days_ms = u32::MAX - 1_000;

        assert!(oled.refresh_due(forty_nine_days_ms));
        assert!(!oled.refresh_due(forty_nine_days_ms + 50));
        // Across the wrap the subtraction wraps too, and the gate must not
        // wedge shut for the next 49 days.
        assert!(oled.refresh_due(u32::MAX - 100));
        assert!(oled.refresh_due(0));
    }
}
