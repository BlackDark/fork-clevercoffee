/**
 * @file PowerOnBehaviour.h
 * @brief Pure boot-state resolver for pid.power_on_behaviour
 */

#pragma once

#include "clevercoffee/defaults.h"
#include "clevercoffee/state/MachineStateIds.h"

namespace CleverCoffee::Core {

struct PowerOnResolution {
    MachineStateId state{MachineStateId::STANDBY};
    bool           runtimePid{false};
    bool           resetStandbyTimer{false};
    bool           persistPid{false};
};

namespace detail {

[[nodiscard]] inline PowerOnResolution heatUp(bool persistPid = false) noexcept {
    return {MachineStateId::PID_NORMAL, true, true, persistPid};
}

[[nodiscard]] inline PowerOnResolution stayOff(MachineStateId state) noexcept {
    return {state, false, false, false};
}

} // namespace detail

[[nodiscard]] inline PowerOnResolution resolvePowerOnBehaviour(bool                      powerSwitchEnabled,
                                                               Hardware::SwitchType      switchType,
                                                               bool                      togglePressed,
                                                               Process::PowerOnBehaviour behaviour,
                                                               bool                      pidEnabled) noexcept {
    if (powerSwitchEnabled && switchType == Hardware::SwitchType::TOGGLE) {
        if (togglePressed) {
            return detail::heatUp(true);
        }
        return detail::stayOff(MachineStateId::PID_DISABLED);
    }

    switch (behaviour) {
        case Process::PowerOnBehaviour::HEAT:
            return detail::heatUp();
        case Process::PowerOnBehaviour::RESTORE:
            if (pidEnabled) {
                return detail::heatUp();
            }
            return detail::stayOff(MachineStateId::STANDBY);
        case Process::PowerOnBehaviour::STANDBY:
        default:
            return detail::stayOff(MachineStateId::STANDBY);
    }
}

} // namespace CleverCoffee::Core
