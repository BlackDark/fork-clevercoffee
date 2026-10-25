//! GPIO transport for [`cc_display::leds::LedOutput`].
//!
//! Inversion applies to pin level, independently of relay polarity. Disabled LEDs
//! have no pin. Steam's GPIO1 belongs to UART0, leaving two driven status LEDs;
//! see [`crate::pins`] and [pins.md](../../../docs/hardware/pins.md).

use esp_idf_hal::gpio::{Level, PinDriver};
use log::warn;

use crate::pins::{BREW_LED, STATUS_LED};

/// The level that lights a common-cathode LED: the anode rises above the cathode.
///
/// This is the "on" side of the comparison; an inverted LED drives the opposite
/// level.
const LIT: Level = Level::High;

/// One LED on one pin: the pin and its polarity.
///
/// "Enabled" is represented by the *absence* of a [`StandardLed`] in [`Leds`],
/// which is a stronger statement than a flag nobody reads — a disabled LED has no
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
        // Every LED starts off. An LED that is off at boot is the one property
        // that matters here: a machine that is powered up mid-brew must not look
        // idle.
        let _ = pin.set_level(Level::Low);
        Self { pin, inverted }
    }

    /// Drive the pin for `state`: `state != inverted` selects HIGH.
    ///
    /// # Errors
    ///
    /// `EspError` from the pin write. Logged and swallowed at the call site
    /// rather than propagated: an LED that cannot be written is a cosmetic
    /// fault, and the control loop must not be brought down by one. This is the
    /// same trade `actuators.rs` makes for its pins.
    pub fn set(&mut self, on: bool) {
        // Written with the operands the other way round because clippy's
        // `nonminimal_bool` is `-D` here. Same truth table: `inverted` inverts the
        // **pin**, so an inverted LED asked to go off drives high.
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
    /// is false. Modelling "no LED" as "no pin" means a disabled LED cannot be
    /// written at all, rather than being guarded at every call site.
    ///
    /// Both pins arrive **already configured as outputs**: a pin that could not
    /// be configured arrives as `None`, with the failure already logged by the
    /// caller. So this cannot fail, does not return a `Result`, and cannot
    /// swallow anything — [`StandardLed::new`] only stores the pin and drives
    /// it low.
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

    /// Apply one display decision: three booleans, two pins.
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
