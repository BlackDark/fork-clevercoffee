/**
 * @file PidStates.cpp
 * @brief Implementation of PID-related state classes
 */

#include "clevercoffee/state/states/PidStates.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/constants/Timing.h"
#include "clevercoffee/hardware/Switch.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/types/GlobalTypes.h"

// PidNormalState Implementation
void PidNormalState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "PID Normal mode active - ready for operation");
    // Don't reset standby timer on entry - only reset on user activity (switch presses)
}

void PidNormalState::onExitImpl(MachineStateContext& context) {
    // Safety: Disable pump when exiting PID normal state (water dispensing cleanup)
    context.disablePump();
    LOG(DEBUG, "PID normal exit - pump disabled");
}

void PidNormalState::update(MachineStateContext& context) {
    // Standby timer is now only reset when switches are pressed (user activity)
    // No automatic periodic reset - this prevents unnecessary resets when standby is disabled

    // FEATURE: Handle water dispensing from water switch in PID mode
    // User can dispense hot water directly without entering HOT_WATER mode
    auto* waterSwitch = context.getHotWaterSwitch();
    if (waterSwitch) {
        if (waterSwitch->isPressed()) {
            // User pressed water switch - activate pump to dispense hot water
            context.enablePump();
            LOGF(DEBUG, "PID: Water dispensing active");
        } else {
            // User released water switch - deactivate pump
            context.disablePump();
        }
    }
}

std::optional<MachineStateId> PidNormalState::checkSpecificTransitions(MachineStateContext& context) {
    // CRITICAL: Check if PID was disabled while in this state
    if (!context.isPidEnabled()) {
        context.logStateTransition(getStateId(), MachineStateId::PID_DISABLED, "PID disabled");
        return MachineStateId::PID_DISABLED;
    }

    if (context.isBrewStartRequested()) {
        context.setBrewStartRequested(false);

        // In manual mode, go directly to BREW_RUNNING (skip preinfusion)
        const bool isAutomatic = context.getConfig().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;
        if (!isAutomatic) {
            context.logStateTransition(
                getStateId(), MachineStateId::BREW_RUNNING, "Brew start requested (manual mode)");
            return MachineStateId::BREW_RUNNING;
        }

        // In automatic mode, start with preinfusion
        context.logStateTransition(
            getStateId(), MachineStateId::BREW_PREINFUSION, "Brew start requested (automatic mode)");
        return MachineStateId::BREW_PREINFUSION;
    }
    // Hot water is handled directly in PID_NORMAL via pump control (no separate state needed)
    if (context.isSteamStartRequested()) {
        context.setSteamStartRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::STEAM_RUNNING, "Steam start requested");
        return MachineStateId::STEAM_RUNNING;
    }
    if (context.isManualFlushStartRequested()) {
        context.setManualFlushStartRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::MANUAL_FLUSH_RUNNING, "Manual flush start requested");
        return MachineStateId::MANUAL_FLUSH_RUNNING;
    }
    if (context.isBackflushStartRequested()) {
        context.setBackflushStartRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush start requested");
        return MachineStateId::BACKFLUSH_IDLE;
    }
    if (context.isStandbyRequested()) {
        context.setStandbyRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::STANDBY, "Standby requested");
        return MachineStateId::STANDBY;
    }
    // Initialize standby timer if needed (when standby is first enabled)
    context.initializeStandbyTimerIfNeeded();
    if (shouldEnterStandby(context)) {
        context.logStateTransition(getStateId(), MachineStateId::STANDBY, "Entering standby mode");
        return MachineStateId::STANDBY;
    }
    return std::nullopt;
}

bool PidNormalState::shouldEnterStandby(MachineStateContext& context) const {
    return context.shouldEnterStandby();
}

void PidNormalState::resetStandbyTimerIfNeeded(MachineStateContext& context) const {
    // Only reset standby timer if standby is enabled
    // This prevents unnecessary resets and log messages when standby is disabled
    if (Config::getInstance().standbyEnabled.get()) {
        context.resetStandbyTimer(getStateId());
    }
}

// PidDisabledState Implementation
void PidDisabledState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "PID disabled - operations without temperature control");
    context.setPidRuntimeState(false);
    // Safety: Disable pump when PID is disabled to prevent runaway water dispensing
    context.disablePump();
}

void PidDisabledState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "PID Disabled: Temp=%.1f°C, PidEnabled=%s",
         context.getCurrentTemperature(),
         context.isPidEnabled() ? "YES" : "NO");
}

std::optional<MachineStateId> PidDisabledState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isPidEnabled()) {
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "PID enabled");
        return MachineStateId::PID_NORMAL;
    }
    return std::nullopt;
}