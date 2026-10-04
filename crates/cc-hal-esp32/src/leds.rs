//! The two status LEDs the firmware actually drives, as GPIO writes.
//!
//! Owner: **3.1** of
//! [`32-findings-2026-10-03.md`](../../../docs/rust-migration/32-findings-2026-10-03.md).
//!
//! # This is the whole of the hardware half
//!
//! The decision of *which* LED should be lit is [`cc_display::leds::LedOutput`],
//! which is pure, host-tested, and lives with the C++'s `displayHelpers.h` port.
//! This module owns the pin and nothing else: given three booleans, it writes two
//! GPIOs. That split is the C++'s own — `LoopManager::updateLEDs` asks
//! (`LoopManager.cpp:255-286`) and `StandardLED` writes
//! (`StandardLED.cpp:16-18`) — and it is what makes the rule reachable by
//! `just test` without a board.
//!
//! # No relay, and why that is a decision rather than an omission
//!
//! An LED is a few milliamps. The C++'s `StandardLED` is a `GPIOPin` and nothing
//! else, wrapped in a `LED` base class whose other three virtuals
//! (`setColor`, `setBrightness`, `setGPIOState`'s RGB siblings) are **empty
//! function bodies** for a standard LED (`StandardLED.cpp:26-33`). There is no
//! driver chip on this board. Introducing a relay driver here would add a
//! flyback diode, a coil and a failure mode to switch a diode, and would make
//! this module the second thing in the firmware that can hold a mains-adjacent
//! handle — which `actuators.rs`'s module doc is at some length about preventing.
//!
//! # The `inverted` handling, transcribed
//!
//! `StandardLED::setGPIOState` (`StandardLED.cpp:16-20`):
//!
//! ```cpp
//! void StandardLED::setGPIOState(const bool state) {
//!     if (enabled) {
//!         gpio.write(state != inverted ? HIGH : LOW);
//!     }
//! }
//! ```
//!
//! Two things are in that one line and both are kept. The polarity is
//! `state != inverted`, so `inverted` inverts the **pin**, not the request — an
//! inverted LED asked to go off drives the pin high. And the `enabled` check is
//! inside the write rather than at the call site, which is why
//! [`Leds::apply`] drives only the LEDs that were configured: a disabled LED has
//! no pin, so there is nothing for the check to guard.
//!
//! Note the C++ reads `inverted` from `hardware.leds.*.inverted`, which the
//! operator sets for a common-anode LED — it is **not** derived from the relay
//! trigger config the way the three relays' polarity is. That is what the brief's
//! phrasing ("an `inverted` flag from the relay trigger config") got wrong, and
//! the C++ is unambiguous: `HardwareManager::initializeLEDs` (`:101, :109, :117`)
//! reads `config_.hardwareLeds{Status,Brew,Steam}Inverted.get()`.
//!
//! # Why there are two LEDs and not three
//!
//! `PIN_STEAMLED` is GPIO1, which this firmware gives to UART0's TXD for the Wi-Fi
//! provisioning console, and `Peripherals::take()` will not hand a pin to two
//! owners. The full reasoning, including what moving it to GPIO32 would cost, is
//! in [`crate::pins`] — it is a long comment there and this is the one-line
//! version. The decision is recorded in `intentional-diffs.md`.

use esp_idf_hal::gpio::{Level, PinDriver};
use log::warn;

use crate::pins::{BREW_LED, STATUS_LED};

/// The level that lights a common-cathode LED: the anode rises above the cathode.
///
/// `StandardLED` writes `HIGH` for "on" unless inverted, so this is the "on"
/// side of the comparison.
const LIT: Level = Level::High;

/// One LED on one pin: `StandardLED`'s entire state.
///
/// The C++'s class holds a `GPIOPin&`, a `bool inverted` and a `bool enabled`
/// (`StandardLED.h:31-34`). Only the first two survive the port: `enabled` is
/// represented by the *absence* of a [`StandardLed`] in [`Leds`], which is a
/// stronger statement than a flag nobody reads — a disabled LED has no
/// [`PinDriver`] and therefore cannot be written at all.
pub struct StandardLed {
    /// The pin. `PinDriver` is the owned handle, so dropping this drives nothing
    /// and freeing it is the only way to release the pin.
    pin: PinDriver<'static, esp_idf_hal::gpio::Output>,
    /// `hardware.leds.<name>.inverted` — for a common-anode LED.
    inverted: bool,
}

impl StandardLed {
    /// Take a pin already configured as an output, and drive it **off**.
    ///
    /// # Errors
    ///
    /// None from this function, but the pin arrives from
    /// [`esp_idf_hal::gpio::PinDriver::output`], which fails if the pin is
    /// already in use. The caller configures it.
    #[must_use]
    pub fn new(mut pin: PinDriver<'static, esp_idf_hal::gpio::Output>, inverted: bool) -> Self {
        // `statusLed_->turnOff()` and its two siblings
        // (`HardwareManager.cpp:103, :111, :119`). An LED that is off at boot is
        // the one property that matters here: a machine that is powered up
        // mid-brew must not look idle.
        let _ = pin.set_level(Level::Low);
        Self { pin, inverted }
    }

    /// `StandardLED::setGPIOState(state)` — `state != inverted ? HIGH : LOW`.
    ///
    /// # Errors
    ///
    /// `EspError` from the pin write. Logged and swallowed at the call site
    /// rather than propagated: an LED that cannot be written is a cosmetic
    /// fault, and the control loop must not be brought down by one. This is the
    /// same trade `actuators.rs` makes for its pins.
    pub fn set(&mut self, on: bool) {
        // The C++'s `state != inverted ? HIGH : LOW`, written with the operands
        // the other way round because clippy's `nonminimal_bool` is `-D` here.
        // Same truth table: `inverted` inverts the **pin**, so an inverted LED
        // asked to go off drives high.
        let level = if on == self.inverted { Level::Low } else { LIT };
        if let Err(err) = self.pin.set_level(level) {
            // `Level` is not `Display`, so it is named rather than interpolated.
            let named = if level == LIT { "HIGH" } else { "LOW" };
            warn!(
                "leds: the LED on GPIO{} could not be driven {named}: {err:?}",
                self.pin.pin()
            );
        }
    }
}

/// The status and brew LEDs, and nothing else.
///
/// Built once, at bring-up, and moved into the control task. Holding them in one
/// struct is what makes "which LEDs does this firmware drive" a single
/// `rg STATUS_LED` answer rather than three locals in `bring_up`.
pub struct Leds {
    /// `PIN_STATUSLED` (GPIO26), or `None` when `hardware.leds.status.enabled`
    /// is false.
    status: Option<StandardLed>,
    /// `PIN_BREWLED` (GPIO19), or `None` when `hardware.leds.brew.enabled` is
    /// false.
    brew: Option<StandardLed>,
}

impl Leds {
    /// Hold the pins the operator asked for.
    ///
    /// `status` and `brew` are `None` when the matching `hardware.leds.*.enabled`
    /// is false — the C++'s `initializeLEDs` (`:95-125`) constructs no
    /// `StandardLED` in that case, and `updateLEDs` then dereferences a null
    /// `unique_ptr` behind an `if`. Modelling "no LED" as "no pin" is the same
    /// rule without the null.
    ///
    /// # Errors
    ///
    /// `EspError` from configuring either pin as an output. Propagated rather
    /// than swallowed, unlike [`StandardLed::set`]: this runs once at bring-up,
    /// before any task exists, and a machine whose LED pin is genuinely unusable
    /// is worth a loud line in the boot log.
    #[must_use]
    pub fn new(
        status: Option<PinDriver<'static, esp_idf_hal::gpio::Output>>,
        brew: Option<PinDriver<'static, esp_idf_hal::gpio::Output>>,
        status_inverted: bool,
        brew_inverted: bool,
    ) -> Self {
        Self {
            status: status.map(|pin| StandardLed::new(pin, status_inverted)),
            brew: brew.map(|pin| StandardLed::new(pin, brew_inverted)),
        }
    }

    /// No LEDs at all — the `hardware.leds.*.enabled = false` case.
    ///
    /// A named constructor rather than a `Default` impl because "no LEDs" is a
    /// configuration an operator chose, and a `Leds::default()` would make it
    /// look like a fallback.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            status: None,
            brew: None,
        }
    }

    /// `updateLEDs`' three writes (`LoopManager.cpp:255-286`).
    ///
    /// `steam` is accepted and **not** acted on. It is a parameter rather than a
    /// dropped one so that the caller passes
    /// [`cc_display::leds::LedOutput`] straight through without unpacking it,
    /// and so that the day GPIO1 is free the change is one line. See
    /// [`crate::pins`] for why it is not free today.
    pub fn apply(&mut self, output: cc_display::leds::LedOutput) {
        if let Some(status) = self.status.as_mut() {
            status.set(output.status);
        }
        if let Some(brew) = self.brew.as_mut() {
            brew.set(output.brew);
        }
    }

    /// Whether any LED is driven, for the bring-up log line.
    #[must_use]
    pub const fn any_configured(&self) -> bool {
        self.status.is_some() || self.brew.is_some()
    }
}

/// The pins this module needs, named so `bring_up` cannot wire them by hand and
/// drift from the map.
pub const STATUS_PIN: u8 = STATUS_LED;
/// See [`STATUS_PIN`].
pub const BREW_PIN: u8 = BREW_LED;
