//! Machine states. The numeric values are the API's `machineState` codes and must not
//! change: the web UI, MQTT consumers and Home Assistant automations key on them.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum State {
    /// Boot: waits for the first temperature reading.
    Init = 0,
    /// Heating to the brew setpoint, ready to brew.
    PidNormal = 20,
    BrewPreinfusion = 31,
    BrewPreinfusionPause = 32,
    BrewRunning = 33,
    /// Shot done; shown briefly, then back to `PidNormal`.
    BrewFinished = 34,
    /// Pump and valve open while the brew switch is held (from backflush mode).
    ManualFlush = 36,
    /// Heating to the steam setpoint.
    Steam = 51,
    BackflushIdle = 60,
    BackflushFilling = 61,
    BackflushFlushing = 62,
    BackflushFinished = 63,
    WaterTankEmpty = 70,
    /// Over-temperature: everything off until the boiler cools down.
    EmergencyStop = 80,
    /// The user turned the PID off.
    PidDisabled = 90,
    Standby = 95,
    /// The temperature sensor stopped delivering valid readings.
    SensorError = 100,
}

impl State {
    pub const fn code(self) -> u8 {
        self as u8
    }

    pub const fn name(self) -> &'static str {
        match self {
            State::Init => "INIT",
            State::PidNormal => "PID_NORMAL",
            State::BrewPreinfusion => "BREW_PREINFUSION",
            State::BrewPreinfusionPause => "BREW_PREINFUSION_PAUSE",
            State::BrewRunning => "BREW_RUNNING",
            State::BrewFinished => "BREW_FINISHED",
            State::ManualFlush => "MANUAL_FLUSH_RUNNING",
            State::Steam => "STEAM_RUNNING",
            State::BackflushIdle => "BACKFLUSH_IDLE",
            State::BackflushFilling => "BACKFLUSH_FILLING",
            State::BackflushFlushing => "BACKFLUSH_FLUSHING",
            State::BackflushFinished => "BACKFLUSH_FINISHED",
            State::WaterTankEmpty => "WATER_TANK_EMPTY",
            State::EmergencyStop => "EMERGENCY_STOP",
            State::PidDisabled => "PID_DISABLED",
            State::Standby => "STANDBY",
            State::SensorError => "SENSOR_ERROR",
        }
    }

    /// Any brew phase, including the finished display.
    pub const fn is_brew(self) -> bool {
        matches!(
            self,
            State::BrewPreinfusion | State::BrewPreinfusionPause | State::BrewRunning | State::BrewFinished
        )
    }

    /// A brew that is still pumping or soaking.
    pub const fn is_brew_active(self) -> bool {
        self.is_brew() && !matches!(self, State::BrewFinished)
    }

    pub const fn is_backflush(self) -> bool {
        matches!(
            self,
            State::BackflushIdle
                | State::BackflushFilling
                | State::BackflushFlushing
                | State::BackflushFinished
        )
    }

    /// States in which water flows through the group: only these may open the valve.
    pub const fn is_water_flow(self) -> bool {
        self.is_brew_active() || matches!(self, State::ManualFlush | State::BackflushFilling)
    }

    /// Operations that need the PID: they abort when the PID is switched off.
    pub const fn needs_pid(self) -> bool {
        !matches!(
            self,
            State::PidDisabled
                | State::PidNormal
                | State::Standby
                | State::Init
                | State::EmergencyStop
                | State::SensorError
                | State::WaterTankEmpty
        )
    }

    /// The heater may run in this state (the PID still decides how much).
    pub const fn allows_heating(self, keep_heater_on_empty_tank: bool) -> bool {
        match self {
            State::WaterTankEmpty => keep_heater_on_empty_tank,
            State::Init | State::PidDisabled | State::SensorError | State::EmergencyStop | State::Standby => {
                false
            }
            s => !s.is_backflush(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_the_api() {
        assert_eq!(State::PidNormal.code(), 20);
        assert_eq!(State::BrewRunning.code(), 33);
        assert_eq!(State::Standby.code(), 95);
        assert_eq!(State::SensorError.code(), 100);
    }

    #[test]
    fn classification_matches_the_cpp() {
        assert!(State::BrewFinished.is_brew());
        assert!(!State::BrewFinished.is_brew_active());
        assert!(State::BrewPreinfusionPause.is_water_flow());
        assert!(!State::BackflushFlushing.is_water_flow());
        assert!(!State::BackflushIdle.allows_heating(false));
        assert!(State::WaterTankEmpty.allows_heating(true));
        assert!(State::Steam.needs_pid());
        assert!(!State::WaterTankEmpty.needs_pid());
    }
}
