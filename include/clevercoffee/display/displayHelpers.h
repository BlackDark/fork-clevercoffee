/**
 * @file displayHelpers.h
 * @brief Shared tolerance and near-setpoint helpers for display and status LED
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/constants/Temperature.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/types/GlobalTypes.h"
#include "clevercoffee/utils/SystemUtils.h"

#include <cmath>

namespace CleverCoffee::Display {

inline MachineStateId getCurrentDisplayState(SystemContext& systemContext) {
    if (!systemContext.machineStateContext()) {
        return MachineStateId::INIT;
    }
    return systemContext.machineStateContext()->getCurrentStateId();
}

inline bool isHeatingLogoConditionMet(const SystemContext& systemContext) {
    if (Config::getInstance().displayHeatingLogo.get() == 0) {
        return false;
    }
    if (getCurrentDisplayState(const_cast<SystemContext&>(systemContext)) != MachineStateId::PID_NORMAL) {
        return false;
    }
    return systemContext.processSetpoint() - systemContext.processTemperature() >
           static_cast<double>(Temperature::HEATING_LOGO_THRESHOLD_C);
}

inline double getDisplayBlinkingDelta() {
    return Config::getInstance().displayBlinkingDelta.get();
}

inline double getStatusLedTolerance(const MachineStateId state) {
    using Temperature::TEMP_TOLERANCE_STEAM_C;
    return isSteamState(state) ? static_cast<double>(TEMP_TOLERANCE_STEAM_C) : getDisplayBlinkingDelta();
}

inline bool isNearSetpointForDisplay(const double temperature, const double setpoint, const double delta) {
    return std::fabs(temperature - setpoint) < delta;
}

inline bool isNearSetpointForDisplay(const double temperature, const double setpoint) {
    return isNearSetpointForDisplay(temperature, setpoint, getDisplayBlinkingDelta());
}

inline bool isNearSetpointForStatusLed(const double temperature, const double setpoint, const double tolerance) {
    return std::fabs(temperature - setpoint) <= tolerance;
}

inline bool isNearSetpointForStatusLed(const double temperature, const double setpoint, const MachineStateId state) {
    return isNearSetpointForStatusLed(temperature, setpoint, getStatusLedTolerance(state));
}

inline bool isBlinkPhaseOn(const SystemContext& systemContext) {
    return systemContext.isrCounter() < 500;
}

} // namespace CleverCoffee::Display
