//! Standardised error codes.
//!
//! Port of `ErrorCode` in `include/clevercoffee/errors/ErrorCodes.h:16-40`.
//! The C++ enum is implicit, so the discriminants are 0..=12 in declaration
//! order; they are stated explicitly here because they cross the wire in API
//! responses and MQTT payloads.

/// A system error, classified.
///
/// The discriminants are positional, matching the implicit C++ enum: inserting
/// or reordering a variant changes the wire format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ErrorCode {
    /// No error. The C++ models this as a code rather than an `Option`, so
    /// `ErrorCode::Success` is the value an `Error` carries when nothing went
    /// wrong.
    Success = 0,
    /// The sensor did not answer in time.
    SensorTimeout = 1,
    /// The sensor is not present on the bus.
    SensorDisconnected = 2,
    /// The sensor reported a fault (e.g. the TSIC-306 CRC sentinels).
    SensorFault = 3,
    /// The sensor has not finished initialising or converting.
    SensorNotReady = 4,
    /// A relay, LED or bus operation failed.
    HardwareFailure = 5,
    /// The water tank is empty and the pump is therefore blocked.
    WaterTankEmpty = 6,
    /// The machine was asked to do something its current state forbids.
    InvalidState = 7,
    /// A state transition that the machine does not allow was requested.
    InvalidTransition = 8,
    /// Emergency stop is latched.
    EmergencyStop = 9,
    /// The over-temperature threshold was crossed.
    EmergencyTemperature = 10,
    /// Something failed and could not be classified.
    UnknownError = 11,
}

impl ErrorCode {
    /// Every code, in discriminant order.
    pub const ALL: [Self; 12] = [
        Self::Success,
        Self::SensorTimeout,
        Self::SensorDisconnected,
        Self::SensorFault,
        Self::SensorNotReady,
        Self::HardwareFailure,
        Self::WaterTankEmpty,
        Self::InvalidState,
        Self::InvalidTransition,
        Self::EmergencyStop,
        Self::EmergencyTemperature,
        Self::UnknownError,
    ];

    /// The numeric code as it appears on the wire.
    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }

    /// The C++ enumerator name, as `Error::codeString()` prints it
    /// (`ErrorCodes.h:152-181`).
    #[must_use]
    pub const fn code_string(self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::SensorTimeout => "SENSOR_TIMEOUT",
            Self::SensorDisconnected => "SENSOR_DISCONNECTED",
            Self::SensorFault => "SENSOR_FAULT",
            Self::SensorNotReady => "SENSOR_NOT_READY",
            Self::HardwareFailure => "HARDWARE_FAILURE",
            Self::WaterTankEmpty => "WATER_TANK_EMPTY",
            Self::InvalidState => "INVALID_STATE",
            Self::InvalidTransition => "INVALID_TRANSITION",
            Self::EmergencyStop => "EMERGENCY_STOP",
            Self::EmergencyTemperature => "EMERGENCY_TEMPERATURE",
            Self::UnknownError => "UNKNOWN_ERROR",
        }
    }

    /// Whether this error requires the machine to stop what it is doing.
    ///
    /// Port of `Error::isCritical()` (`ErrorCodes.h:142-146`), which marks
    /// exactly five codes. A critical error is one where continuing to heat or
    /// pump risks the machine, not merely where a retry might help.
    #[must_use]
    pub const fn is_critical(self) -> bool {
        matches!(
            self,
            Self::SensorDisconnected
                | Self::SensorFault
                | Self::HardwareFailure
                | Self::EmergencyStop
                | Self::EmergencyTemperature
        )
    }

    /// A user-facing description with a recovery suggestion, from
    /// `Error::getUserMessage()` (`ErrorCodes.h:78-104`).
    #[must_use]
    pub const fn user_message(self) -> &'static str {
        match self {
            Self::Success => "No error",
            Self::SensorTimeout => "Sensor timeout - check wiring and power",
            Self::SensorDisconnected => "Sensor disconnected - check connections",
            Self::SensorFault => "Sensor fault detected - check wiring or replace sensor",
            Self::SensorNotReady => "Sensor not ready - wait for initialization",
            Self::HardwareFailure => "Hardware failure - check power and connections",
            Self::WaterTankEmpty => "Water tank empty - please refill",
            Self::InvalidState => "Invalid system state - restart may be required",
            Self::InvalidTransition => {
                "Invalid state transition - system will recover automatically"
            }
            Self::EmergencyStop => "EMERGENCY STOP - temperature too high, system disabled",
            Self::EmergencyTemperature => "EMERGENCY - temperature critical, system disabled",
            Self::UnknownError => "Unknown error occurred",
        }
    }

    /// Actionable recovery steps, from `Error::getRecoverySuggestion()`
    /// (`ErrorCodes.h:110-136`).
    #[must_use]
    pub const fn recovery_suggestion(self) -> &'static str {
        match self {
            Self::Success => "No action needed",
            Self::SensorTimeout => {
                "Check sensor wiring, ensure power is connected, wait 10s and retry"
            }
            Self::SensorDisconnected => {
                "Verify sensor is properly connected, check for loose wires"
            }
            Self::SensorFault => "Inspect sensor wiring for damage, replace sensor if needed",
            Self::SensorNotReady => "Wait for system initialization to complete (usually < 5s)",
            Self::HardwareFailure => {
                "Check all power connections, verify hardware is properly installed"
            }
            Self::WaterTankEmpty => {
                "Refill water tank - system will resume automatically when full"
            }
            Self::InvalidState => "Restart the system - press reset button or power cycle",
            Self::InvalidTransition => "No action needed - system will recover automatically",
            Self::EmergencyStop => "Wait for temperature to cool below 100°C, then restart system",
            Self::EmergencyTemperature => {
                "CRITICAL - Allow system to cool, check for hardware issues before restart"
            }
            Self::UnknownError => "Check system logs for details, restart if problem persists",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discriminants_match_the_implicit_cpp_enum() {
        // ErrorCodes.h:16-40 declares the variants with no explicit values, so
        // they are 0..=11 in order.
        for (i, code) in ErrorCode::ALL.iter().enumerate() {
            assert_eq!(usize::from(code.code()), i, "{}", code.code_string());
        }
    }

    #[test]
    fn code_strings_match_the_cpp_switch() {
        for code in ErrorCode::ALL {
            assert!(!code.code_string().is_empty());
            assert_eq!(code.code_string(), code.code_string().to_uppercase());
        }
        assert_eq!(ErrorCode::EmergencyStop.code_string(), "EMERGENCY_STOP");
    }

    #[test]
    fn criticality_matches_the_cpp_is_critical() {
        for code in ErrorCode::ALL {
            let expected = matches!(
                code,
                ErrorCode::SensorDisconnected
                    | ErrorCode::SensorFault
                    | ErrorCode::HardwareFailure
                    | ErrorCode::EmergencyStop
                    | ErrorCode::EmergencyTemperature
            );
            assert_eq!(code.is_critical(), expected, "{}", code.code_string());
        }
    }

    #[test]
    fn every_code_has_a_message_and_a_suggestion() {
        for code in ErrorCode::ALL {
            assert!(!code.user_message().is_empty());
            assert!(!code.recovery_suggestion().is_empty());
        }
    }
}
