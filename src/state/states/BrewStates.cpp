/**
 * @file BrewStates.cpp
 * @brief Implementation of brew-related state classes
 */

#include "clevercoffee/state/states/BrewStates.h"

#include "clevercoffee/types/GlobalTypes.h"
#include "clevercoffee/constants/Timing.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/state/MachineStateContext.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/control/ProcessController.h"
#include "clevercoffee/Config.h"
#include "clevercoffee/defaults.h"


// BrewStates Implementation
void BrewPreinfusionState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew preinfusion started");
    // Enable pump and open valve for preinfusion
    context.enablePump();
    context.openWaterValve();
    // Initialize brew time tracking
    if (context.systemContext().processController()) {
        context.systemContext().processController()->setCurrBrewTime(0.0);
    }
}

void BrewPreinfusionState::onExitImpl(MachineStateContext& context) {
    // Safety: Ensure pump and valve are disabled when exiting preinfusion
    cleanupPumpAndValve(context);
    LOG(DEBUG, "Brew preinfusion exit - hardware cleaned up");
}

void BrewPreinfusionState::update(MachineStateContext& context) {
    // Keep pump and valve enabled during preinfusion
    context.enablePump();
    context.openWaterValve();
    
    // Update brew time (elapsed time since state entry)
    if (context.systemContext().processController()) {
        const unsigned long elapsedMs = context.getStateElapsedTimeMs();
        context.systemContext().processController()->setCurrBrewTime(static_cast<double>(elapsedMs));
    }
    
    LOGF(DEBUG,
         "Brew Preinfusion: Temp=%.1f°C, Pressure=%.1fbar, Weight=%.1fg",
         context.getCurrentTemperature(),
         context.getFilteredPressure(),
         context.getCurrentBrewWeight());
}

std::optional<MachineStateId> BrewPreinfusionState::checkSpecificTransitions(MachineStateContext& context) {
    // Check for brew stop request (flag-based - handlers set this flag)
    if (auto pidState = checkBrewStopRequest(context)) {
        return pidState;
    }
    
    // In manual mode, skip preinfusion entirely - go directly to running
    const bool isAutomatic = context.getConfig().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;
    if (!isAutomatic) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_RUNNING, "Manual mode - skipping preinfusion");
        return MachineStateId::BREW_RUNNING;
    }
    
    // Automatic mode: Check if preinfusion is enabled
    const bool preinfusionEnabled = context.getConfig().brewPreInfusionEnabled.get();
    
    if (!preinfusionEnabled) {
        // Preinfusion is disabled - transition directly to running immediately
        context.logStateTransition(getStateId(), MachineStateId::BREW_RUNNING, "Preinfusion disabled, starting brew");
        return MachineStateId::BREW_RUNNING;
    }
    
    // Preinfusion is enabled - check if preinfusion time has elapsed
    const double preinfusionTimeSec = context.getPreInfusionTime();
    const unsigned long preinfusionTimeMs = static_cast<unsigned long>(preinfusionTimeSec * 1000.0);
    
    if (context.hasStateTimeoutElapsed(preinfusionTimeMs)) {
        // Preinfusion time elapsed - check if pause is configured
        const double pauseTimeSec = context.getConfig().brewPreInfusionPause.get();
        
        if (pauseTimeSec > 0.0) {
            // Pause is configured - transition to pause state
            context.logStateTransition(getStateId(), MachineStateId::BREW_PREINFUSION_PAUSE, "Preinfusion time elapsed, entering pause");
            return MachineStateId::BREW_PREINFUSION_PAUSE;
        } else {
            // No pause - transition directly to running
            context.logStateTransition(getStateId(), MachineStateId::BREW_RUNNING, "Preinfusion time elapsed, starting brew");
            return MachineStateId::BREW_RUNNING;
        }
    }
    
    return std::nullopt;
}

void BrewPreinfusionPauseState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew preinfusion pause - blooming phase");
    // Ensure pump and valve are closed during pause
    cleanupPumpAndValve(context);
    // Store preinfusion time before pause
    if (context.systemContext().processController()) {
        const double preinfusionTimeMs = context.getPreInfusionTime() * 1000.0;
        context.systemContext().processController()->setCurrBrewTime(preinfusionTimeMs);
    }
}

void BrewPreinfusionPauseState::onExitImpl(MachineStateContext& context) {
    // Safety: Ensure pump and valve are disabled when exiting preinfusion pause
    cleanupPumpAndValve(context);
    LOG(DEBUG, "Brew preinfusion pause exit - hardware cleaned up");
}

void BrewPreinfusionPauseState::update(MachineStateContext& context) {
    // Ensure pump and valve remain closed during pause
    context.disablePump();
    context.closeWaterValve();
    
    // Update brew time (preinfusion time + pause elapsed time)
    if (context.systemContext().processController()) {
        const double preinfusionTimeMs = context.getPreInfusionTime() * 1000.0;
        const unsigned long pauseElapsedMs = context.getStateElapsedTimeMs();
        context.systemContext().processController()->setCurrBrewTime(preinfusionTimeMs + static_cast<double>(pauseElapsedMs));
    }
    
    LOGF(DEBUG,
         "Brew Preinfusion Pause: Temp=%.1f°C, Pressure=%.1fbar, Weight=%.1fg",
         context.getCurrentTemperature(),
         context.getFilteredPressure(),
         context.getCurrentBrewWeight());
}

std::optional<MachineStateId> BrewPreinfusionPauseState::checkSpecificTransitions(MachineStateContext& context) {
    // Check for brew stop request (flag-based - handlers set this flag)
    if (auto pidState = checkBrewStopRequest(context)) {
        return pidState;
    }
    
    // Safety check: In manual mode, skip pause (shouldn't reach here, but just in case)
    const bool isAutomatic = context.getConfig().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;
    if (!isAutomatic) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_RUNNING, "Manual mode - skipping pause");
        return MachineStateId::BREW_RUNNING;
    }
    
    // Check if pause time has elapsed
    const double pauseTimeSec = context.getConfig().brewPreInfusionPause.get();
    const unsigned long pauseTimeMs = static_cast<unsigned long>(pauseTimeSec * 1000.0);
    
    if (context.hasStateTimeoutElapsed(pauseTimeMs)) {
        // Pause time elapsed - transition to running
        context.logStateTransition(getStateId(), MachineStateId::BREW_RUNNING, "Preinfusion pause completed, starting brew");
        return MachineStateId::BREW_RUNNING;
    }
    
    return std::nullopt;
}

void BrewRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew running - active brewing in progress");
    // Enable pump and open valve for brewing
    context.enablePump();
    context.openWaterValve();
    
    // Initialize brew time tracking
    if (context.systemContext().processController()) {
        const bool isAutomatic = context.getConfig().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;
        
        double totalBrewTimeMs = 0.0;
        // In automatic mode, include preinfusion + pause time if applicable
        if (isAutomatic) {
            const bool preinfusionEnabled = context.getConfig().brewPreInfusionEnabled.get();
            if (preinfusionEnabled) {
                totalBrewTimeMs += context.getPreInfusionTime() * 1000.0;
                const double pauseTimeSec = context.getConfig().brewPreInfusionPause.get();
                if (pauseTimeSec > 0.0) {
                    totalBrewTimeMs += pauseTimeSec * 1000.0;
                }
            }
        }
        // In manual mode, start from 0 (no preinfusion)
        
        context.systemContext().processController()->setCurrBrewTime(totalBrewTimeMs);
        
        // Set total target brew time if brew by time is enabled (automatic mode only)
        // Total target = preinfusion + pause + target brew time
        const bool brewByTime = context.getConfig().brewByTimeEnabled.get();
        if (isAutomatic && brewByTime) {
            double totalTargetTimeMs = 0.0;
            
            // Add preinfusion time if enabled
            const bool preinfusionEnabled = context.getConfig().brewPreInfusionEnabled.get();
            if (preinfusionEnabled) {
                totalTargetTimeMs += context.getPreInfusionTime() * 1000.0;
                // Add pause time if configured
                const double pauseTimeSec = context.getConfig().brewPreInfusionPause.get();
                if (pauseTimeSec > 0.0) {
                    totalTargetTimeMs += pauseTimeSec * 1000.0;
                }
            }
            
            // Add target brew time
            const double targetBrewTimeSec = context.getTargetBrewTime();
            totalTargetTimeMs += targetBrewTimeSec * 1000.0;
            
            context.systemContext().processController()->setTotalTargetBrewTime(totalTargetTimeMs);
        } else {
            // Clear target time in manual mode or if brew by time is disabled
            context.systemContext().processController()->setTotalTargetBrewTime(0.0);
        }
    }
}

void BrewRunningState::onExitImpl(MachineStateContext& context) {
    // Safety: Ensure pump and valve are disabled when exiting brew running state
    context.disablePump();
    context.closeWaterValve();
    LOG(DEBUG, "Brew running exit - hardware cleaned up");
}

void BrewRunningState::update(MachineStateContext& context) {
    // Keep pump and valve enabled during brewing
    context.enablePump();
    context.openWaterValve();
    
    // Update brew time (base time + elapsed time in running state)
    if (context.systemContext().processController()) {
        const bool isAutomatic = context.getConfig().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;
        
        double baseBrewTimeMs = 0.0;
        // In automatic mode, include preinfusion + pause time if applicable
        if (isAutomatic) {
            const bool preinfusionEnabled = context.getConfig().brewPreInfusionEnabled.get();
            if (preinfusionEnabled) {
                baseBrewTimeMs += context.getPreInfusionTime() * 1000.0;
                const double pauseTimeSec = context.getConfig().brewPreInfusionPause.get();
                if (pauseTimeSec > 0.0) {
                    baseBrewTimeMs += pauseTimeSec * 1000.0;
                }
            }
        }
        // In manual mode, base time is 0 (no preinfusion)
        
        const unsigned long runningElapsedMs = context.getStateElapsedTimeMs();
        const double totalBrewTimeMs = baseBrewTimeMs + static_cast<double>(runningElapsedMs);
        context.systemContext().processController()->setCurrBrewTime(totalBrewTimeMs);
    }
    
    LOGF(DEBUG,
         "Brew Running: Weight=%.1fg, Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentBrewWeight(),
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

std::optional<MachineStateId> BrewRunningState::checkSpecificTransitions(MachineStateContext& context) {
    // Check for brew stop request (flag-based - handlers set this flag)
    if (context.isBrewStopRequested()) {
        context.setBrewStopRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::BREW_FINISHED, "Brew stop requested");
        return MachineStateId::BREW_FINISHED;
    }
    
    // Check automatic brew stop conditions (only in automatic mode)
    const bool isAutomatic = context.getConfig().brewMode.get() == Process::BrewMode::AUTOMATIC_BREW;
    
    if (isAutomatic) {
        // Check brew by time
        const bool brewByTime = context.getConfig().brewByTimeEnabled.get();
        if (brewByTime && context.systemContext().processController()) {
            const double currentBrewTime = context.systemContext().processCurrentBrewTime();
            const double targetBrewTime = context.systemContext().processTotalTargetBrewTime();
            if (targetBrewTime > 0.0 && currentBrewTime >= targetBrewTime) {
                context.logStateTransition(getStateId(), MachineStateId::BREW_FINISHED, "Target brew time reached");
                return MachineStateId::BREW_FINISHED;
            }
        }
        
        // Check brew by weight
        const bool brewByWeight = context.getConfig().brewByWeightEnabled.get();
        if (brewByWeight) {
            const float currentWeight = context.getCurrentBrewWeight();
            const double targetWeight = context.getConfig().brewByWeightTargetWeight.get();
            if (targetWeight > 0.0 && currentWeight >= static_cast<float>(targetWeight)) {
                context.logStateTransition(getStateId(), MachineStateId::BREW_FINISHED, "Target brew weight reached");
                return MachineStateId::BREW_FINISHED;
            }
        }
    }
    
    return std::nullopt;
}

void BrewFinishedState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew cycle completed");
}

void BrewFinishedState::onExitImpl(MachineStateContext& context) {
    // Safety: Ensure pump and valve are disabled when exiting brew finished state
    context.disablePump();
    context.closeWaterValve();
    LOG(DEBUG, "Brew finished exit - hardware cleaned up");
}

void BrewFinishedState::update(MachineStateContext& context) {
    LOGF(DEBUG,
         "Brew Finished: Weight=%.1fg, Temp=%.1f°C",
         context.getCurrentBrewWeight(),
         context.getCurrentTemperature());
}

std::optional<MachineStateId> BrewFinishedState::checkSpecificTransitions(MachineStateContext& context) {
    // Check for brew start request (flag-based - handlers set this flag when switch is activated)
    if (context.isBrewStartRequested()) {
        context.setBrewStartRequested(false);
        context.logStateTransition(getStateId(), MachineStateId::BREW_PREINFUSION, "Brew start requested after finished");
        return MachineStateId::BREW_PREINFUSION;
    }
    
    // Use a hardcoded 3 second timeout for the finished state (display time)
    if (context.hasStateTimeoutElapsed(CleverCoffee::BrewTiming::FINISHED_DISPLAY_TIMEOUT_MS)) {
        // After timeout, return to PID state
        // If user wants to start new brew, they'll press switch and handler will set flag
        return transitionToPidState(context, "Brew finished display timeout");
    }

    return std::nullopt;
}