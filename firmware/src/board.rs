//! Pin assignment of the ESP32 DevKitC v4 CleverCoffee board (unchanged from the C++
//! firmware's `pinmapping.h`, so existing PCBs keep working).
//!
//! GPIO34–39 are input-only and have no internal pulls: the switches need external
//! pull resistors. GPIO1 is UART0 TX; the steam LED uses it only when enabled, which
//! silences the serial log.

/// Pins of optional parts stay unused in builds without them.
#[allow(dead_code)]
pub struct Pins {
    pub power_switch: esp_hal::peripherals::GPIO39<'static>,
    pub brew_switch: esp_hal::peripherals::GPIO34<'static>,
    pub steam_switch: esp_hal::peripherals::GPIO35<'static>,
    pub water_switch: esp_hal::peripherals::GPIO36<'static>,
    pub temp_sensor: esp_hal::peripherals::GPIO16<'static>,
    pub water_tank: esp_hal::peripherals::GPIO23<'static>,
    pub hx_data: esp_hal::peripherals::GPIO32<'static>,
    pub hx_data2: esp_hal::peripherals::GPIO25<'static>,
    pub hx_clock: esp_hal::peripherals::GPIO33<'static>,
    pub valve: esp_hal::peripherals::GPIO17<'static>,
    pub pump: esp_hal::peripherals::GPIO27<'static>,
    pub heater: esp_hal::peripherals::GPIO2<'static>,
    pub status_led: esp_hal::peripherals::GPIO26<'static>,
    pub brew_led: esp_hal::peripherals::GPIO19<'static>,
    pub steam_led: esp_hal::peripherals::GPIO1<'static>,
    pub sda: esp_hal::peripherals::GPIO21<'static>,
    pub scl: esp_hal::peripherals::GPIO22<'static>,
}

/// Splits the pins out of the peripherals (the rest stays available).
#[macro_export]
macro_rules! take_pins {
    ($p:ident) => {
        $crate::board::Pins {
            power_switch: $p.GPIO39,
            brew_switch: $p.GPIO34,
            steam_switch: $p.GPIO35,
            water_switch: $p.GPIO36,
            temp_sensor: $p.GPIO16,
            water_tank: $p.GPIO23,
            hx_data: $p.GPIO32,
            hx_data2: $p.GPIO25,
            hx_clock: $p.GPIO33,
            valve: $p.GPIO17,
            pump: $p.GPIO27,
            heater: $p.GPIO2,
            status_led: $p.GPIO26,
            brew_led: $p.GPIO19,
            steam_led: $p.GPIO1,
            sda: $p.GPIO21,
            scl: $p.GPIO22,
        }
    };
}
