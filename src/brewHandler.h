/**
 * @file brewHandler.h
 *
 * @brief Handler for brewing
 *
 */
// TODO:
//  show sections on website only if needed
//  add pressure to shot timer?
//  backflush also as bool, enable from website over diffrent var
//  SteamOn also as bool, rethink enable from website

#pragma once

#include "Config.h"
#include "Logger.h"
#include "brewStates.h"
#include "hardware/Relay.h"
#include "hardware/Switch.h"
#include "state/GlobalState.h"
#include "state/MachineState.h"

// Backward compatibility reference
#include "scaleHandler.h"

bool isPowerSwitchOperationAllowed();

/**
 * @brief True if in an intermediate brew state, false if idle or finished (injected version)
 */
inline bool isBrewStateActive(BrewState brewState) {
    return (brewState != kBrewIdle && brewState != kBrewFinished);
}

/**
 * @brief True if in an intermediate brew state, false if idle or finished
 */
inline bool checkBrewActive() {
    return (g_state.sensors.currBrewState != kBrewIdle && g_state.sensors.currBrewState != kBrewFinished);
}

/**
 * @brief True if in a machine state related to brew or flush, false if in other states (injected version)
 */
inline bool isMachineStateBrewRelated(LegacyMachineState machineState) {
    return (machineState == LegacyMachineState::kEmergencyStop || machineState == LegacyMachineState::kBackflush || machineState == LegacyMachineState::kManualFlush);
}

/**
 * @brief True if in a machine state related to brew or flush, false if in other states
 */
inline bool checkBrewStates() {
    return (g_state.machine.machineState == LegacyMachineState::kEmergencyStop || g_state.machine.machineState == LegacyMachineState::kBackflush || g_state.machine.machineState == LegacyMachineState::kManualFlush);
}

/**
 * @brief turns off valve if not in an active brew state or if machine state changes away from one related to brewing or flushing (injected version)
 */
inline void performValveSafetyShutdown(BrewState& brewState, LegacyMachineState machineState, Relay* valveRelay) {
    if (!isBrewStateActive(brewState) && !isMachineStateBrewRelated(machineState)) {
        brewState = kBrewIdle; // reset state to idle if not in an active brew/flush state
        if (valveRelay) {
            valveRelay->off();
        }
    }
}

/**
 * @brief turns off valve if not in an active brew state or if g_state.machine.machineState changes away from one related to brewing or flushing
 */
inline void valveSafetyShutdownCheck() {
    if (!checkBrewActive() && !checkBrewStates()) {
        g_state.sensors.currBrewState == kBrewIdle; // reset state to idle if not in an active brew/flush state
        g_state.hardware.valveRelay->off();
    }
}

/**
 * @brief Toggle or momentary input for Brew Switch
 */
inline void checkBrewSwitch() {
    if (!isPowerSwitchOperationAllowed()) {
        return;
    }

    static bool loggedEmptyWaterTank = false;
    g_state.sensors.brewSwitchReading = reinterpret_cast<Switch*>(g_state.hardware.brewSwitch)->isPressed();

    // Block brewSwitch input when water tank is empty
    if (g_state.machine.machineState == LegacyMachineState::kWaterTankEmpty) {

        if (!loggedEmptyWaterTank && (g_state.sensors.currBrewSwitchState == kBrewSwitchIdle || g_state.sensors.currBrewSwitchState == kBrewSwitchPressed)) {
            LOG(WARNING, "Brew switch input ignored: Water tank empty");
            loggedEmptyWaterTank = true;
        }
        return;
    }

    // Block brewSwitch input while hot water is being drawn
    if (g_state.machine.machineState == LegacyMachineState::kHotWater) {
        return;
    }

    loggedEmptyWaterTank = false;

    // Convert toggle brew switch input to brew switch state
    if (const int brewSwitchType = static_cast<int>(Config::getInstance().hardwareSwitchesBrewType.get()); brewSwitchType == Switch::TOGGLE) {
        if (g_state.sensors.currReadingBrewSwitch != g_state.sensors.brewSwitchReading) {
            g_state.sensors.currReadingBrewSwitch = g_state.sensors.brewSwitchReading;
        }

        switch (g_state.sensors.currBrewSwitchState) {
            case kBrewSwitchIdle:
                if (g_state.sensors.currReadingBrewSwitch == HIGH) {
                    g_state.sensors.currBrewSwitchState = kBrewSwitchShortPressed;
                    LOG(DEBUG, "Toggle Brew switch is ON -> got to g_state.sensors.currBrewSwitchState = kBrewSwitchShortPressed");
                }
                break;

            case kBrewSwitchShortPressed:
                if (g_state.sensors.currReadingBrewSwitch == LOW) {
                    g_state.sensors.currBrewSwitchState = kBrewSwitchIdle;
                    LOG(DEBUG, "Toggle Brew switch is OFF -> got to g_state.sensors.currBrewSwitchState = kBrewSwitchIdle");
                }
                else if (g_state.sensors.currBrewState == kBrewFinished || g_state.sensors.currBackflushState == kBackflushFinished) {
                    g_state.sensors.currBrewSwitchState = kBrewSwitchWaitForRelease;
                    LOG(DEBUG, "Brew reached target or backflush done -> got to g_state.sensors.currBrewSwitchState = kBrewSwitchWaitForRelease");
                }
                break;

            case kBrewSwitchWaitForRelease:
                if (g_state.sensors.currReadingBrewSwitch == LOW) {
                    g_state.sensors.currBrewSwitchState = kBrewSwitchIdle;
                    LOG(DEBUG, "Brew switch reset -> got to g_state.sensors.currBrewSwitchState = kBrewSwitchIdle");
                }
                break;

            default:

                g_state.sensors.currBrewSwitchState = kBrewSwitchIdle;
                LOG(DEBUG, "Unexpected switch state -> g_state.sensors.currBrewSwitchState = kBrewSwitchIdle");
                break;
        }
    }
    // Convert momentary brew switch input to brew switch state
    else if (brewSwitchType == Switch::MOMENTARY) {
        if (g_state.sensors.currReadingBrewSwitch != g_state.sensors.brewSwitchReading) {
            g_state.sensors.currReadingBrewSwitch = g_state.sensors.brewSwitchReading;
        }

        switch (g_state.sensors.currBrewSwitchState) {
            case kBrewSwitchIdle:
                if (g_state.sensors.currReadingBrewSwitch == HIGH) {
                    g_state.sensors.currBrewSwitchState = kBrewSwitchPressed;
                    LOG(DEBUG, "Brew switch press detected -> got to g_state.sensors.currBrewSwitchState = kBrewSwitchPressed");
                }
                break;

            case kBrewSwitchPressed:                                // Brew switch pressed - check for short or long press
                if (g_state.sensors.currReadingBrewSwitch == LOW) { // Brew switch short press detected
                    g_state.sensors.currBrewSwitchState = kBrewSwitchShortPressed;
                    LOG(DEBUG, "Brew switch short press detected -> got to g_state.sensors.currBrewSwitchState = kBrewSwitchShortPressed; start brew");
                }
                else if (g_state.sensors.currReadingBrewSwitch == HIGH && reinterpret_cast<Switch*>(g_state.hardware.brewSwitch)->longPressDetected()) { // Brew switch long press detected
                    g_state.sensors.currBrewSwitchState = kBrewSwitchLongPressed;
                    LOG(DEBUG, "Brew switch long press detected -> got to g_state.sensors.currBrewSwitchState = kBrewSwitchLongPressed; start manual flush");
                }
                break;

            case kBrewSwitchShortPressed:
                if (g_state.sensors.currReadingBrewSwitch == HIGH) { // Brew switch short press detected while brew is running - abort brew
                    g_state.sensors.currBrewSwitchState = kBrewSwitchWaitForRelease;
                    LOG(DEBUG, "Brew switch short press detected -> got to g_state.sensors.currBrewSwitchState = kBrewSwitchWaitForRelease; brew or backflush stopped manually");
                }
                else if (g_state.sensors.currBrewState == kBrewFinished || g_state.sensors.currBackflushState == kBackflushFinished) { // Brew reached target and stopped or blackflush cycle done
                    g_state.sensors.currBrewSwitchState = kBrewSwitchWaitForRelease;
                    LOG(DEBUG, "Brew reached target or backflush done -> got to g_state.sensors.currBrewSwitchState = kBrewSwitchWaitForRelease");
                }
                break;

            case kBrewSwitchLongPressed:
                if (g_state.sensors.currReadingBrewSwitch == LOW) { // Brew switch got released after long press detected - reset brewswitch
                    g_state.sensors.currBrewSwitchState = kBrewSwitchWaitForRelease;
                    LOG(DEBUG, "Brew switch long press released -> got to g_state.sensors.currBrewSwitchState = kBrewSwitchWaitForRelease; stop manual flush");
                }
                break;

            case kBrewSwitchWaitForRelease: // wait for brew switch got released
                if (g_state.sensors.currReadingBrewSwitch == LOW) {
                    g_state.sensors.currBrewSwitchState = kBrewSwitchIdle;
                    LOG(DEBUG, "Brew switch reset -> got to g_state.sensors.currBrewSwitchState = kBrewSwitchIdle");
                }
                break;

            default:
                g_state.sensors.currBrewSwitchState = kBrewSwitchIdle;
                LOG(DEBUG, "Unexpected switch state -> g_state.sensors.currBrewSwitchState = kBrewSwitchIdle");
                break;
        }
    }
}

/**
 * @brief If set to publish debug messages then list what the current action is and what triggered it
 * @return void
 */
inline void debugPumpState(String label, String state) {
    g_state.debug.hotWaterStateDebug = state;
    IFLOG(DEBUG) {
        if (g_state.debug.hotWaterStateDebug != g_state.debug.lastHotWaterStateDebug) {
            LOGF(DEBUG, "Hot water state: %s - BrewHandler: %s", g_state.debug.hotWaterStateDebug, label);
            g_state.debug.lastHotWaterStateDebug = g_state.debug.hotWaterStateDebug;
        }
    }
}

/**
 * @brief Brew process handeling including timer and state machine for brew-by-time and brew-by-weight
 * @return true if brew is running, false otherwise
 */
inline bool brew() {
    if (!Config::getInstance().hardwareSwitchesBrewEnabled.get() || reinterpret_cast<Switch*>(g_state.hardware.brewSwitch) == nullptr) {
        return false; // brew switch is not enabled, so no brew process running
    }

    const unsigned long currentMillisTemp = millis();
    checkBrewSwitch();

    // abort function for state machine from every state
    if (g_state.sensors.currBrewSwitchState == kBrewSwitchIdle && g_state.sensors.currBrewState > kBrewIdle && g_state.sensors.currBrewState < kBrewFinished) {
        if (g_state.sensors.currBrewState != kBrewFinished) {
            LOG(INFO, "Brew stopped manually");
        }
        g_state.sensors.currBrewState = kBrewFinished;
    }
    // calculated brew time while brew is running
    if (g_state.sensors.currBrewState > kBrewIdle && g_state.sensors.currBrewState < kBrewFinished) {
        g_state.process.currBrewTime = currentMillisTemp - g_state.process.startingTime;
    }

    const Process::BrewMode brewMode = Config::getInstance().brewMode.get();
    const bool brewByTimeEnabled = brewMode != Process::BrewMode::MANUAL_BREW && Config::getInstance().brewByTimeEnabled.get();
    const bool brewByWeightEnabled = brewMode != Process::BrewMode::MANUAL_BREW && Config::getInstance().brewByWeightEnabled.get();
    const bool preinfusionEnabled = Config::getInstance().brewPreInfusionEnabled.get();

    // check if brewswitch was turned off after a brew; Brew only runs once even brewswitch is still pressed
    if (g_state.sensors.currBrewSwitchState == kBrewSwitchIdle) {
        g_state.sensors.brewSwitchWasOff = true;
    }

    // set brew time every cycle, in case changes are done during brew
    if (Config::getInstance().brewByTimeTargetTime.get() > 0) {
        g_state.process.totalTargetBrewTime = Config::getInstance().brewByTimeTargetTime.get() * 1000;

        if (preinfusionEnabled) {
            g_state.process.totalTargetBrewTime += Config::getInstance().brewPreInfusionTime.get() * 1000 + Config::getInstance().brewPreInfusionPause.get() * 1000;
        }
    }
    else {
        // Stop by time deactivated --> g_state.process.totalTargetBrewTime = 0
        g_state.process.totalTargetBrewTime = 0;
    }

    // state machine for brew
    switch (g_state.sensors.currBrewState) {
        case kBrewIdle:                             // waiting step for brew switch turning on
            if (g_state.sensors.currBrewSwitchState == kBrewSwitchShortPressed && g_state.sensors.brewSwitchWasOff && !g_state.machine.backflushOn && g_state.machine.machineState != LegacyMachineState::kBackflush) {
                g_state.process.startingTime = millis();
                g_state.process.currBrewTime = 0;   // reset g_state.process.currBrewTime, last brew is still stored
                g_state.sensors.currBrewWeight = 0; // reset currBrewWeight for new brew

                LOG(INFO, "Brew started");

                if (!preinfusionEnabled) {
                    LOG(INFO, "Brew running");
                    g_state.sensors.currBrewState = kBrewRunning;
                }
                else {
                    LOG(INFO, "Preinfusion running");
                    g_state.sensors.currBrewState = kPreinfusion;
                }

                if (Config::getInstance().hardwareSensorsScaleEnabled.get() && Config::getInstance().hardwareSensorsScaleType.get() == Hardware::ScaleType::BLUETOOTH && Config::getInstance().brewByWeightEnabled.get() &&
                    Config::getInstance().brewByWeightAutoTare.get()) {
                    LOG(INFO, "Tare scale");

                    if (g_state.hardware.scale) {
                        g_state.hardware.scale->tare();

                        // Mark that auto-tare is in progress for Bluetooth scales
                        g_state.sensors.autoTareInProgress = true;
                        g_state.sensors.autoTareStartTime = millis();
                    }
                }
            }

            break;

        case kPreinfusion:
            g_state.hardware.valveRelay->on();
            g_state.hardware.pumpRelay->on();
            debugPumpState("Preinfusion", "on");

            if (g_state.process.currBrewTime > Config::getInstance().brewPreInfusionTime.get() * 1000) {
                LOG(INFO, "Preinfusion pause running");
                g_state.sensors.currBrewState = kPreinfusionPause;
            }

            break;

        case kPreinfusionPause:
            g_state.hardware.valveRelay->on();
            g_state.hardware.pumpRelay->off();
            debugPumpState("Pause", "off");

            if (g_state.process.currBrewTime > (Config::getInstance().brewPreInfusionTime.get() + Config::getInstance().brewPreInfusionPause.get()) * 1000) {
                LOG(INFO, "Brew running");
                g_state.sensors.currBrewState = kBrewRunning;
            }

            break;

        case kBrewRunning:
            {
                g_state.hardware.valveRelay->on();
                g_state.hardware.pumpRelay->on();
                debugPumpState("BrewRunning", "on");

                const auto targetBrewWeight = Config::getInstance().brewByWeightTargetWeight.get();

                if (g_state.process.currBrewTime > g_state.process.totalTargetBrewTime && brewByTimeEnabled) {
                    LOG(INFO, "Brew reached time target");
                    g_state.sensors.currBrewState = kBrewFinished;
                }
                else if (Config::getInstance().hardwareSensorsScaleEnabled.get() && g_state.sensors.currBrewWeight > targetBrewWeight && brewByWeightEnabled) {
                    LOG(INFO, "Brew reached weight target");
                    g_state.sensors.currBrewState = kBrewFinished;
                }

                break;
            }

        case kBrewFinished:
            g_state.hardware.valveRelay->off();
            g_state.hardware.pumpRelay->off();
            debugPumpState("BrewFinished", "off");

            g_state.sensors.brewSwitchWasOff = false;
            LOG(INFO, "Brew finished");
            LOGF(INFO, "Shot time: %4.1f s", g_state.process.currBrewTime / 1000);
            LOG(INFO, "Brew idle");
            g_state.sensors.currBrewState = kBrewIdle;

            break;

        default:
            g_state.sensors.currBrewState = kBrewIdle;
            LOG(DEBUG, "Unexpected brew state -> g_state.sensors.currBrewState = kBrewIdle");

            break;
    }

    return checkBrewActive();
}

/**
 * @brief manual grouphead flush
 * @return true if manual flush is running, false otherwise
 */
inline bool manualFlush() {
    if (!Config::getInstance().hardwareSwitchesBrewEnabled.get() || reinterpret_cast<Switch*>(g_state.hardware.brewSwitch) == nullptr) {
        return false; // brew switch is not enabled, so no brew process running
    }

    const unsigned long currentMillisTemp = millis();
    checkBrewSwitch();

    if (g_state.sensors.currManualFlushState == kManualFlushRunning) {
        g_state.process.currBrewTime = currentMillisTemp - g_state.process.startingTime;
    }

    switch (g_state.sensors.currManualFlushState) {
        case kManualFlushIdle:
            if (g_state.sensors.currBrewSwitchState == kBrewSwitchLongPressed) {
                g_state.process.startingTime = millis();
                g_state.hardware.valveRelay->on();
                g_state.hardware.pumpRelay->on();
                debugPumpState("ManualFlush", "on");
                LOG(INFO, "Manual flush started");
                g_state.sensors.currManualFlushState = kManualFlushRunning;
            }
            break;

        case kManualFlushRunning:
            if (g_state.sensors.currBrewSwitchState != kBrewSwitchLongPressed) {
                g_state.hardware.valveRelay->off();
                g_state.hardware.pumpRelay->off();
                debugPumpState("ManualFlush", "off");
                LOG(INFO, "Manual flush stopped");
                LOGF(INFO, "Manual flush time: %4.1f s", g_state.process.currBrewTime / 1000);
                g_state.sensors.currManualFlushState = kManualFlushIdle;
            }
            break;

        default:
            g_state.sensors.currManualFlushState = kManualFlushIdle;
            LOG(DEBUG, "Unexpected manual flush state -> g_state.sensors.currManualFlushState = kManualFlushIdle");

            break;
    }

    return g_state.sensors.currManualFlushState == kManualFlushRunning;
}

/**
 * @brief Backflush
 */
inline void backflush() {
    if (!Config::getInstance().hardwareSwitchesBrewEnabled.get() || reinterpret_cast<Switch*>(g_state.hardware.brewSwitch) == nullptr) {
        return; // brew switch is not enabled, so no brew process running
    }

    checkBrewSwitch();

    if (g_state.sensors.currBackflushState != kBackflushIdle && !g_state.machine.backflushOn) {
        g_state.sensors.currBackflushState = kBackflushFinished; // Force reset in case g_state.machine.backflushOn is reset during backflush!
        LOG(INFO, "Backflush: Disabled via webinterface");
    }
    else if (g_state.network.offlineMode || g_state.sensors.currBrewState > kBrewIdle || Config::getInstance().backflushCycles.get() <= 0 || !g_state.machine.backflushOn) {
        return;
    }

    // abort function for state machine from every state
    if (g_state.sensors.currBrewSwitchState == kBrewSwitchIdle && g_state.sensors.currBackflushState > kBackflushIdle && g_state.sensors.currBackflushState < kBackflushFinished) {
        g_state.sensors.currBackflushState = kBackflushFinished;

        LOG(INFO, "Backflush stopped manually");
    }

    // check if brewswitch was turned off after a backflush; Backflush only runs once even brewswitch is still pressed
    if (g_state.sensors.currBrewSwitchState == kBrewSwitchIdle) {
        g_state.sensors.brewSwitchWasOff = true;
    }

    // State machine for backflush
    switch (g_state.sensors.currBackflushState) {
        case kBackflushIdle:
            if (g_state.sensors.currBrewSwitchState == kBrewSwitchShortPressed && g_state.machine.backflushOn && g_state.sensors.brewSwitchWasOff) {
                g_state.process.startingTime = millis();
                g_state.hardware.valveRelay->on();
                g_state.hardware.pumpRelay->on();
                debugPumpState("Backflush", "on");
                LOGF(INFO, "Start backflush cycle %d", g_state.machine.currBackflushCycles);
                LOG(INFO, "Backflush: filling portafilter");
                g_state.sensors.currBackflushState = kBackflushFilling;
            }

            break;

        case kBackflushFilling:
            if (millis() - g_state.process.startingTime > Config::getInstance().backflushFillTime.get() * 1000) {
                g_state.process.startingTime = millis();
                g_state.hardware.valveRelay->off();
                g_state.hardware.pumpRelay->off();
                debugPumpState("Backflush", "off");
                LOG(INFO, "Backflush: flushing into drip tray");
                g_state.sensors.currBackflushState = kBackflushFlushing;
            }
            break;

        case kBackflushFlushing:
            if (millis() - g_state.process.startingTime > Config::getInstance().backflushFlushTime.get() * 1000) {
                if (g_state.machine.currBackflushCycles < Config::getInstance().backflushCycles.get()) {
                    g_state.process.startingTime = millis();
                    g_state.hardware.valveRelay->on();
                    g_state.hardware.pumpRelay->on();
                    debugPumpState("Backflush", "on");
                    g_state.machine.currBackflushCycles++;
                    LOGF(INFO, "Backflush: next backflush cycle %d", g_state.machine.currBackflushCycles);
                    LOG(INFO, "Backflush: filling portafilter");
                    g_state.sensors.currBackflushState = kBackflushFilling;
                }
                else {
                    g_state.sensors.currBackflushState = kBackflushFinished;
                }
            }
            break;

        case kBackflushFinished:
            g_state.hardware.valveRelay->off();
            g_state.hardware.pumpRelay->off();
            debugPumpState("Backflush", "off");
            LOGF(INFO, "Backflush finished after %d cycles", g_state.machine.currBackflushCycles);
            g_state.machine.currBackflushCycles = 1;
            g_state.sensors.brewSwitchWasOff = false;
            g_state.sensors.currBackflushState = kBackflushIdle;

            break;

        default:
            g_state.sensors.currBackflushState = kBackflushIdle;
            LOG(DEBUG, "Unexpected backflush state -> g_state.sensors.currBackflushState = kBackflushIdle");

            break;
    }
}
