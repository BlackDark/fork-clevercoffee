/**
 * @file AllStates.cpp
 * @brief Consolidated implementation for all state machine states
 */

#include "clevercoffee/state/BaseState.h"
#include "clevercoffee/state/states/AllStates.h"
#include "clevercoffee/state/MachineStateContext.h"

#include "clevercoffee/Logger.h"

// BackflushStates Implementation
void BackflushState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Backflush idle - ready for backflush operation");
}

void BackflushState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Backflush Idle: Temp=%.1f°C", context.getCurrentTemperature());
}

std::unique_ptr<MachineState> BackflushState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBackflushStart) {
        flags.requestBackflushStart = false;
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_FILLING, "Backflush start requested");
        return std::make_unique<BackflushFillingState>();
    }
    return nullptr;
}

void BackflushFillingState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Backflush Filling: Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}



void BackflushFlushingState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Backflush Flushing: Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}



std::unique_ptr<MachineState> BackflushFlushingState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBackflushStop) {
        flags.requestBackflushStop = false;
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush stop requested");
        return std::make_unique<BackflushState>();
    }
    return TimedState<MachineStateId::BACKFLUSH_FLUSHING, BackflushFlushingState, BackflushFinishedState>::checkSpecificTransitions(context);
}

void BackflushFinishedState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Backflush finished - backflush cycle complete");
}

void BackflushFinishedState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Backflush Finished: Temp=%.1f°C", context.getCurrentTemperature());
}



// BrewStates Implementation
void BrewIdleState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew idle - ready to start brewing");
}

void BrewIdleState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Brew Idle: Temp=%.1f°C, Weight=%.1fg, Tank=%s",
         context.getCurrentTemperature(),
         context.getCurrentBrewWeight(),
         context.isWaterTankFull() ? "OK" : "EMPTY");
}

std::unique_ptr<MachineState> BrewIdleState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBrewStart) {
        flags.requestBrewStart = false;
        context.logStateTransition(getStateId(), MachineStateId::BREW_PREINFUSION, "Brew start requested");
        return std::make_unique<BrewPreinfusionState>();
    }
    if (context.isBrewActive()) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_PREINFUSION, "Brew switch activated");
        return std::make_unique<BrewPreinfusionState>();
    }
    if (context.isBackflushActive() || flags.requestBackflushStart) {
        if (flags.requestBackflushStart) {
            flags.requestBackflushStart = false;
        }
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush activated");
        return std::make_unique<BackflushState>();
    }
    return nullptr;
}

void BrewPreinfusionState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew preinfusion started");
}

void BrewPreinfusionState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Brew Preinfusion: Temp=%.1f°C, Pressure=%.1fbar, Weight=%.1fg",
         context.getCurrentTemperature(),
         context.getFilteredPressure(),
         context.getCurrentBrewWeight());
}

std::unique_ptr<MachineState> BrewPreinfusionState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBrewStop) {
        flags.requestBrewStop = false;
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew stop requested during preinfusion");
        return std::make_unique<BrewIdleState>();
    }
    if (!context.isBrewActive()) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew switch deactivated during preinfusion");
        return std::make_unique<BrewIdleState>();
    }
    return nullptr;
}

void BrewPreinfusionPauseState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew preinfusion pause - blooming phase");
}

void BrewPreinfusionPauseState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Brew Preinfusion Pause: Temp=%.1f°C, Pressure=%.1fbar, Weight=%.1fg",
         context.getCurrentTemperature(),
         context.getFilteredPressure(),
         context.getCurrentBrewWeight());
}

std::unique_ptr<MachineState> BrewPreinfusionPauseState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBrewStop) {
        flags.requestBrewStop = false;
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew stop requested during preinfusion pause");
        return std::make_unique<BrewIdleState>();
    }
    if (!context.isBrewActive()) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew switch deactivated during preinfusion pause");
        return std::make_unique<BrewIdleState>();
    }
    return nullptr;
}

void BrewRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew running - active brewing in progress");
}

void BrewRunningState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Brew Running: Weight=%.1fg, Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentBrewWeight(),
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

std::unique_ptr<MachineState> BrewRunningState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestBrewStop) {
        flags.requestBrewStop = false;
        context.logStateTransition(getStateId(), MachineStateId::BREW_FINISHED, "Brew stop requested");
        return std::make_unique<BrewFinishedState>();
    }
    if (!context.isBrewActive()) {
        context.logStateTransition(getStateId(), MachineStateId::BREW_FINISHED, "Brew completed");
        return std::make_unique<BrewFinishedState>();
    }
    return nullptr;
}

void BrewFinishedState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Brew cycle completed");
}

void BrewFinishedState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Brew Finished: Weight=%.1fg, Temp=%.1f°C",
         context.getCurrentBrewWeight(),
         context.getCurrentTemperature());
}



// EmergencyStopState Implementation
void EmergencyStopState::onEntryImpl(MachineStateContext& context) {
    LOG(ERROR, "EMERGENCY STOP ACTIVATED - System entering safe mode");
    performEmergencyShutdown(context);
    LOGF(ERROR, "Emergency conditions: Temp=%.1f°C, EmergencyStop=%s", context.getCurrentTemperature(), context.isEmergencyStop() ? "ACTIVE" : "INACTIVE");
}

void EmergencyStopState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Emergency stop cleared - System ready for restart");
}

void EmergencyStopState::update(MachineStateContext& context) {
    LOGF(INFO, "Emergency Stop Active: Temp=%.1f°C, Emergency=%s", context.getCurrentTemperature(), context.isEmergencyStop() ? "ACTIVE" : "CLEARED");
    performEmergencyShutdown(context);
}

std::unique_ptr<MachineState> EmergencyStopState::checkSpecificTransitions(MachineStateContext& context) {
    if (isEmergencyCleared(context)) {
        context.logStateTransition(getStateId(), MachineStateId::INIT, "Emergency condition cleared - restarting");
        return getRecoveryState(context);
    }
    return nullptr;
}

void EmergencyStopState::performEmergencyShutdown(MachineStateContext& context) {
    context.performSafeShutdown();
    context.setPidRuntimeState(false);
}

bool EmergencyStopState::isEmergencyCleared(MachineStateContext& context) const {
    if (context.isEmergencyStop()) {
        return false;
    }
    double currentTemp = context.getCurrentTemperature();
    const double SAFE_TEMPERATURE_THRESHOLD = 100.0;
    if (currentTemp > SAFE_TEMPERATURE_THRESHOLD) {
        LOGF(WARNING, "Temperature still elevated: %.1f°C", currentTemp);
        return false;
    }
    return true;
}

std::unique_ptr<MachineState> EmergencyStopState::getRecoveryState(MachineStateContext& context) const {
    return std::make_unique<InitState>();
}

// ErrorStates Implementation
void SensorErrorState::onEntryImpl(MachineStateContext& context) {
    LOG(ERROR, "Sensor error detected - entering safe mode");
    context.enterSafeMode();
    errorStartTime_ = millis();
    recoveryAttempts_++;
    LOGF(INFO, "Sensor error recovery attempt %u/%u", recoveryAttempts_, MAX_RECOVERY_ATTEMPTS);
}

void SensorErrorState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Sensor error resolved - exiting safe mode");
    context.exitSafeMode();
}

void SensorErrorState::update(MachineStateContext& context) {
    unsigned long currentTime = millis();
    unsigned long errorDuration = currentTime - errorStartTime_;
    LOGF(DEBUG, "Sensor Error: Duration=%lums, Recovery=%s", errorDuration,
         context.hasSensorError() ? "PENDING" : "RESOLVED");
}

std::unique_ptr<MachineState> SensorErrorState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during sensor error");
        return std::make_unique<EmergencyStopState>();
    }
    if (!context.hasSensorError() && !context.hasTemperatureError()) {
        unsigned long errorDuration = millis() - errorStartTime_;
        constexpr unsigned long RECOVERY_DELAY_MS = 5000;
        if (errorDuration > RECOVERY_DELAY_MS) {
            if (context.isPidEnabled()) {
                return std::make_unique<PidNormalState>();
            } else {
                return std::make_unique<PidDisabledState>();
            }
        }
    } else {
        errorStartTime_ = millis();
    }
    unsigned long errorDuration = millis() - errorStartTime_;
    constexpr unsigned long MAX_ERROR_DURATION_MS = 60000;
    if (errorDuration > MAX_ERROR_DURATION_MS || recoveryAttempts_ >= MAX_RECOVERY_ATTEMPTS) {
        const char* reason = (recoveryAttempts_ >= MAX_RECOVERY_ATTEMPTS) ?
            "Too many sensor recovery attempts - disabling PID for safety" :
            "Persistent sensor error - disabling PID for safety";
        context.logStateTransition(getStateId(), MachineStateId::PID_DISABLED, reason);
        context.setPidRuntimeState(false);
        return std::make_unique<PidDisabledState>();
    }
    return nullptr;
}

void WaterTankEmptyState::onEntryImpl(MachineStateContext& context) {
    LOG(WARNING, "Water tank empty - please refill");
}

void WaterTankEmptyState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Water Tank Empty: Tank=%s, Temp=%.1f°C",
         context.isWaterTankFull() ? "FILLED" : "EMPTY",
         context.getCurrentTemperature());
}

std::unique_ptr<MachineState> WaterTankEmptyState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during water tank empty");
        return std::make_unique<EmergencyStopState>();
    }
    if (context.isWaterTankFull()) {
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "Water tank refilled");
        if (context.isPidEnabled()) {
            return std::make_unique<PidNormalState>();
        } else {
            return std::make_unique<PidDisabledState>();
        }
    }
    return nullptr;
}

void EepromErrorState::onEntryImpl(MachineStateContext& context) {
    LOG(ERROR, "EEPROM error detected - configuration may be corrupted");
    context.enterSafeMode();
    context.setPidRuntimeState(false);
}

void EepromErrorState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "EEPROM error resolved - configuration restored");
    context.exitSafeMode();
}

void EepromErrorState::update(MachineStateContext& context) {
    LOGF(DEBUG, "EEPROM Error: Configuration storage unavailable");
}

std::unique_ptr<MachineState> EepromErrorState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isEmergencyStop()) {
        context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop during EEPROM error");
        return std::make_unique<EmergencyStopState>();
    }
    static unsigned long eepromErrorStartTime = 0;
    if (eepromErrorStartTime == 0) {
        eepromErrorStartTime = millis();
    }
    constexpr unsigned long EEPROM_RECOVERY_TIMEOUT = 300000;
    if (millis() - eepromErrorStartTime > EEPROM_RECOVERY_TIMEOUT) {
        eepromErrorStartTime = 0;
        context.logStateTransition(getStateId(), MachineStateId::PID_DISABLED, "EEPROM recovery timeout - attempting recovery");
        return std::make_unique<PidDisabledState>();
    }
    return nullptr;
}

// InitState Implementation
void InitState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "System initializing - performing startup checks");
}

void InitState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Init state: Water tank: %s, Sensors: %s, PID: %s", checkWaterTank(context) ? "OK" : "EMPTY", checkSensors(context) ? "OK" : "ERROR", checkPidConfig(context) ? "ENABLED" : "DISABLED");
}

std::unique_ptr<MachineState> InitState::checkSpecificTransitions(MachineStateContext& context) {
    if (!checkPidConfig(context)) {
        context.logStateTransition(getStateId(), MachineStateId::PID_DISABLED, "PID disabled");
        return std::make_unique<PidDisabledState>();
    } else {
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "PID enabled - entering normal operation");
        return std::make_unique<PidNormalState>();
    }
}

bool InitState::checkWaterTank(MachineStateContext& context) const {
    return context.isWaterTankFull();
}

bool InitState::checkSensors(MachineStateContext& context) const {
    if (context.hasSensorError() || context.hasTemperatureError()) {
        return false;
    }
    return true;
}

bool InitState::checkPidConfig(MachineStateContext& context) const {
    return context.isPidEnabled();
}

// PidNormalState Implementation
void PidNormalState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "PID Normal mode active - ready for operation");
    resetStandbyTimerIfNeeded(context);
}

void PidNormalState::update(MachineStateContext& context) {
    resetStandbyTimerIfNeeded(context);
}

std::unique_ptr<MachineState> PidNormalState::checkSpecificTransitions(MachineStateContext& context) {
    if (g_state.machine.flags.requestBrewStart) {
        g_state.machine.flags.requestBrewStart = false;
        context.logStateTransition(getStateId(), MachineStateId::BREW_IDLE, "Brew start requested");
        return std::make_unique<BrewIdleState>();
    }
    if (g_state.machine.flags.requestHotWaterStart) {
        g_state.machine.flags.requestHotWaterStart = false;
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_IDLE, "Hot water start requested");
        return std::make_unique<HotWaterIdleState>();
    }
    if (g_state.machine.flags.requestSteamStart) {
        g_state.machine.flags.requestSteamStart = false;
        context.logStateTransition(getStateId(), MachineStateId::STEAM_IDLE, "Steam start requested");
        return std::make_unique<SteamIdleState>();
    }
    if (g_state.machine.flags.requestManualFlushStart) {
        g_state.machine.flags.requestManualFlushStart = false;
        context.logStateTransition(getStateId(), MachineStateId::MANUAL_FLUSH_IDLE, "Manual flush start requested");
        return std::make_unique<ManualFlushIdleState>();
    }
    if (g_state.machine.flags.requestBackflushStart) {
        g_state.machine.flags.requestBackflushStart = false;
        context.logStateTransition(getStateId(), MachineStateId::BACKFLUSH_IDLE, "Backflush start requested");
        return std::make_unique<BackflushState>();
    }
    if (g_state.machine.flags.requestStandby) {
        g_state.machine.flags.requestStandby = false;
        context.logStateTransition(getStateId(), MachineStateId::STANDBY, "Standby requested");
        return std::make_unique<StandbyState>();
    }
    if (shouldEnterStandby(context)) {
        context.logStateTransition(getStateId(), MachineStateId::STANDBY, "Entering standby mode");
        return std::make_unique<StandbyState>();
    }
    return nullptr;
}

bool PidNormalState::shouldEnterStandby(MachineStateContext& context) const {
    return context.shouldEnterStandby();
}

void PidNormalState::resetStandbyTimerIfNeeded(MachineStateContext& context) const {
    context.resetStandbyTimer(getStateId());
}

// SystemStates Implementation
void PidDisabledState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "PID disabled - operations without temperature control");
    context.setPidRuntimeState(false);
}

void PidDisabledState::update(MachineStateContext& context) {
    LOGF(DEBUG, "PID Disabled: Temp=%.1f°C, PidEnabled=%s",
         context.getCurrentTemperature(),
         context.isPidEnabled() ? "YES" : "NO");
}

std::unique_ptr<MachineState> PidDisabledState::checkSpecificTransitions(MachineStateContext& context) {
    if (context.isPidEnabled()) {
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "PID enabled");
        return std::make_unique<PidNormalState>();
    }
    return nullptr;
}

void StandbyState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Entering standby mode - reducing power consumption");
    context.enterStandbyMode();
    context.setPidRuntimeState(false);
}

void StandbyState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Exiting standby mode - resuming normal operation");
    context.exitStandbyMode();
    context.setPidRuntimeState(true);
}

void StandbyState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Standby: Power saving active, UserActivity=%s, Sensors=%s",
         context.hasUserActivity() ? "DETECTED" : "IDLE",
         context.hasSensorError() ? "ERROR" : "OK");
}

std::unique_ptr<MachineState> StandbyState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestNormalOperation) {
        flags.requestNormalOperation = false;
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "Normal operation requested");
        context.resetMqttReconnectCount();
        return std::make_unique<PidNormalState>();
    }
    if (context.hasUserActivity() || context.shouldExitStandby()) {
        context.logStateTransition(getStateId(), MachineStateId::PID_NORMAL, "User activity detected - exiting standby");
        context.resetMqttReconnectCount();
        return std::make_unique<PidNormalState>();
    }
    return nullptr;
}

void ManualFlushIdleState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Manual flush mode activated");
    context.setManualFlushState(true);
}

void ManualFlushIdleState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Exiting manual flush mode");
    context.setManualFlushState(false);
}

void ManualFlushIdleState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Manual Flush: Temp=%.1f°C, Tank=%s, FlushActive=%s",
         context.getCurrentTemperature(),
         context.isWaterTankFull() ? "OK" : "EMPTY",
         context.isManualFlushActive() ? "YES" : "NO");
}

std::unique_ptr<MachineState> ManualFlushIdleState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestManualFlushStop) {
        flags.requestManualFlushStop = false;
        if (context.isPidEnabled()) {
            return std::make_unique<PidNormalState>();
        } else {
            return std::make_unique<PidDisabledState>();
        }
    }
    if (!context.isManualFlushActive()) {
        if (context.isPidEnabled()) {
            return std::make_unique<PidNormalState>();
        } else {
            return std::make_unique<PidDisabledState>();
        }
    }
    return nullptr;
}

void ManualFlushRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Manual flush running - hardware controlled by handlers");
}

void ManualFlushRunningState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Manual Flush Running: Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

std::unique_ptr<MachineState> ManualFlushRunningState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestManualFlushStop) {
        flags.requestManualFlushStop = false;
        context.logStateTransition(getStateId(), MachineStateId::MANUAL_FLUSH_IDLE, "Manual flush stop requested");
        return std::make_unique<ManualFlushIdleState>();
    }
    if (!context.isManualFlushActive()) {
        context.logStateTransition(getStateId(), MachineStateId::MANUAL_FLUSH_IDLE, "Manual flush deactivated");
        return std::make_unique<ManualFlushIdleState>();
    }
    return nullptr;
}

// WaterSteamStates Implementation
void HotWaterIdleState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Hot water idle - ready to dispense hot water");
}

void HotWaterIdleState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Hot Water Idle: Temp=%.1f°C, Tank=%s, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.isWaterTankFull() ? "OK" : "EMPTY",
         context.getFilteredPressure());
}

std::unique_ptr<MachineState> HotWaterIdleState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestHotWaterStart) {
        flags.requestHotWaterStart = false;
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_RUNNING, "Hot water start requested");
        return std::make_unique<HotWaterRunningState>();
    }
    if (context.isHotWaterActive()) {
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_RUNNING, "Hot water switch activated");
        return std::make_unique<HotWaterRunningState>();
    }
    return nullptr;
}

void HotWaterRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Hot water running - dispensing hot water");
}

void HotWaterRunningState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Hot Water Running: Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

std::unique_ptr<MachineState> HotWaterRunningState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestHotWaterStop) {
        flags.requestHotWaterStop = false;
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_STOPPED, "Hot water stop requested");
        return std::make_unique<HotWaterStoppedState>();
    }
    if (!context.isHotWaterActive()) {
        context.logStateTransition(getStateId(), MachineStateId::HOT_WATER_STOPPED, "Hot water deactivated");
        return std::make_unique<HotWaterStoppedState>();
    }
    return nullptr;
}

void HotWaterStoppedState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Hot water dispensing stopped");
}

void HotWaterStoppedState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Hot Water Stopped: Temp=%.1f°C", context.getCurrentTemperature());
}



void SteamIdleState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Steam mode activated");
    context.setSteamMode(true);
}

void SteamIdleState::onExitImpl(MachineStateContext& context) {
    LOG(INFO, "Exiting steam mode");
    context.setSteamMode(false);
}

void SteamIdleState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Steam: Temp=%.1f°C, Tank=%s, SteamActive=%s",
         context.getCurrentTemperature(),
         context.isWaterTankFull() ? "OK" : "EMPTY",
         context.isSteamActive() ? "YES" : "NO");
}

std::unique_ptr<MachineState> SteamIdleState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestSteamStop) {
        flags.requestSteamStop = false;
        if (context.isPidEnabled()) {
            return std::make_unique<PidNormalState>();
        } else {
            return std::make_unique<PidDisabledState>();
        }
    }
    if (!context.isSteamActive()) {
        if (context.isPidEnabled()) {
            return std::make_unique<PidNormalState>();
        } else {
            return std::make_unique<PidDisabledState>();
        }
    }
    return nullptr;
}

void SteamRunningState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Steam running - actively steaming");
}

void SteamRunningState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Steam Running: Temp=%.1f°C, Pressure=%.1fbar",
         context.getCurrentTemperature(),
         context.getFilteredPressure());
}

std::unique_ptr<MachineState> SteamRunningState::checkSpecificTransitions(MachineStateContext& context) {
    auto& flags = g_state.machine.flags;
    if (flags.requestSteamStop) {
        flags.requestSteamStop = false;
        context.logStateTransition(getStateId(), MachineStateId::STEAM_STOPPED, "Steam stop requested");
        return std::make_unique<SteamStoppedState>();
    }
    if (!context.isSteamActive()) {
        context.logStateTransition(getStateId(), MachineStateId::STEAM_STOPPED, "Steam deactivated");
        return std::make_unique<SteamStoppedState>();
    }
    return nullptr;
}

void SteamStoppedState::onEntryImpl(MachineStateContext& context) {
    LOG(INFO, "Steam stopped - steaming complete");
}

void SteamStoppedState::update(MachineStateContext& context) {
    LOGF(DEBUG, "Steam Stopped: Temp=%.1f°C", context.getCurrentTemperature());
}



template<MachineStateId StateId, typename DerivedState>
std::unique_ptr<MachineState> BaseState<StateId, DerivedState>::checkTransitions(MachineStateContext& context) {
    // Handle common safety transitions first
    // (but only for non-safety states to avoid circular transitions)
    if constexpr (StateId != MachineStateId::EMERGENCY_STOP &&
                    StateId != MachineStateId::SENSOR_ERROR &&
                    StateId != MachineStateId::WATER_TANK_EMPTY &&
                    StateId != MachineStateId::EEPROM_ERROR) {
        if (context.isEmergencyStop()) {
            context.logStateTransition(getStateId(), MachineStateId::EMERGENCY_STOP, "Emergency stop activated");
            return std::make_unique<EmergencyStopState>();
        }
        if (context.hasSensorError()) {
            context.logStateTransition(getStateId(), MachineStateId::SENSOR_ERROR, "Sensor error detected");
            return std::make_unique<SensorErrorState>();
        }
        if (!context.isWaterTankFull()) {
            context.logStateTransition(getStateId(), MachineStateId::WATER_TANK_EMPTY, "Water tank empty detected");
            return std::make_unique<WaterTankEmptyState>();
        }
    }

    // Let derived class handle specific transitions
    return checkSpecificTransitions(context);
}

// Explicit template instantiations
template class BaseState<MachineStateId::BACKFLUSH_IDLE, BackflushState>;
template class BaseState<MachineStateId::BACKFLUSH_FILLING, BackflushFillingState>;
template class BaseState<MachineStateId::BACKFLUSH_FLUSHING, BackflushFlushingState>;
template class BaseState<MachineStateId::BACKFLUSH_FINISHED, BackflushFinishedState>;
template class BaseState<MachineStateId::BREW_IDLE, BrewIdleState>;
template class BaseState<MachineStateId::BREW_PREINFUSION, BrewPreinfusionState>;
template class BaseState<MachineStateId::BREW_PREINFUSION_PAUSE, BrewPreinfusionPauseState>;
template class BaseState<MachineStateId::BREW_RUNNING, BrewRunningState>;
template class BaseState<MachineStateId::BREW_FINISHED, BrewFinishedState>;
template class BaseState<MachineStateId::EMERGENCY_STOP, EmergencyStopState>;
template class BaseState<MachineStateId::SENSOR_ERROR, SensorErrorState>;
template class BaseState<MachineStateId::WATER_TANK_EMPTY, WaterTankEmptyState>;
template class BaseState<MachineStateId::EEPROM_ERROR, EepromErrorState>;
template class BaseState<MachineStateId::INIT, InitState>;
template class BaseState<MachineStateId::PID_NORMAL, PidNormalState>;
template class BaseState<MachineStateId::PID_DISABLED, PidDisabledState>;
template class BaseState<MachineStateId::STANDBY, StandbyState>;
template class BaseState<MachineStateId::MANUAL_FLUSH_IDLE, ManualFlushIdleState>;
template class BaseState<MachineStateId::MANUAL_FLUSH_RUNNING, ManualFlushRunningState>;
template class BaseState<MachineStateId::HOT_WATER_IDLE, HotWaterIdleState>;
template class BaseState<MachineStateId::HOT_WATER_RUNNING, HotWaterRunningState>;
template class BaseState<MachineStateId::HOT_WATER_STOPPED, HotWaterStoppedState>;
template class BaseState<MachineStateId::STEAM_IDLE, SteamIdleState>;
template class BaseState<MachineStateId::STEAM_RUNNING, SteamRunningState>;
template class BaseState<MachineStateId::STEAM_STOPPED, SteamStoppedState>;