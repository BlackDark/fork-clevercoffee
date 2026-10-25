//! Hardware option vocabulary.
//!
//! These are the enumerations behind the `hardware.*` configuration parameters, so
//! `cc-config` names them and `cc-safety` validates against one of them.

from_raw! {
    /// The electrical behaviour of a momentary or toggle switch.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[repr(i8)]
    pub enum SwitchType {
        /// Returns to rest when released.
        Momentary = 0,
        /// Retains its position.
        Toggle = 1,
    }
}

from_raw! {
    /// Whether a switch or float sensor reads open or closed at rest.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[repr(i8)]
    pub enum SwitchMode {
        /// Conducting when at rest.
        NormallyOpen = 0,
        /// Not conducting when at rest.
        NormallyClosed = 1,
    }
}

from_raw! {
    /// Which relay-coil level energises a relay.
    ///
    /// SAFETY: `LowTrigger` cannot be made safe for the heater. An ESP32 GPIO is
    /// high-impedance while the pin is being configured and during reset, so a
    /// low-trigger relay board — which energises its coil when the input is low or
    /// floating — turns the heater **on** at every boot, before any firmware runs.
    /// `cc-safety`'s `validate_config` refuses this value for the heater relay.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[repr(i8)]
    pub enum RelayTriggerType {
        /// Energise by driving the pin low. Unsafe for the heater.
        LowTrigger = 0,
        /// Energise by driving the pin high. The only safe choice for the heater.
        HighTrigger = 1,
    }
}

from_raw! {
    /// The OLED controller family.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[repr(i8)]
    pub enum OledType {
        /// SSD1306, 128×64, 1 KB framebuffer.
        Ssd1306 = 0,
        /// SH1106, 128×64 with a 132-column controller.
        Sh1106 = 1,
    }
}

from_raw! {
    /// The OLED's I²C address.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[repr(i8)]
    pub enum OledAddress {
        /// `0x3C`, the default for most modules.
        Addr3c = 0,
        /// `0x3D`, the alternative some modules ship with.
        Addr3d = 1,
    }
}

impl OledAddress {
    /// The 7-bit I²C address to probe.
    #[must_use]
    pub const fn address(self) -> u8 {
        match self {
            Self::Addr3c => 0x3C,
            Self::Addr3d => 0x3D,
        }
    }
}

from_raw! {
    /// The temperature probe fitted to the machine.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[repr(i8)]
    pub enum TemperatureSensorType {
        /// Espressif `TSIC-306` over `ZACwire`.
        Tsic306 = 0,
        /// Dallas DS18B20 over 1-Wire.
        ///
        /// This is the probe fitted to the machine this firmware was built for: the boot log
        /// reports `sensor: DS18B20 at 0x41af78cdaa376928 (family 0x28)`.
        DallasDs18b20 = 1,
    }
}

from_raw! {
    /// The scale implementation.
    ///
    /// Only the two HX711 arms are implemented; the Bluetooth variant is carried so the
    /// configuration schema can still be read and validated, and the scale bring-up refuses it
    /// by name rather than reporting 0 g.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[repr(i8)]
    pub enum ScaleType {
        /// Two integrated load cells.
        Hx711Dual = 0,
        /// One integrated load cell.
        Hx711Single = 1,
        /// A Bluetooth Low Energy scale. Never constructed.
        Bluetooth = 2,
    }
}
