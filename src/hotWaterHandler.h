/**
 * @file hotWaterHandler.h
 *
 * @brief Handler for digital hot water switch
 */

#pragma once

#include "Config.h"
#include "hardware/Relay.h"
#include "hardware/Switch.h"
#include "state/GlobalState.h"
#include "state/MachineState.h"

enum HotWaterSwitchState {
    kHotWaterSwitchIdle = 10,
    kHotWaterSwitchPressed = 20,
    kHotWaterSwitchShortPressed = 30,
    kHotWaterSwitchLongPressed = 40,
    kHotWaterSwitchWaitForRelease = 50
};

enum HotWaterState {
    kHotWaterIdle = 10,
    kHotWaterRunning = 20,
    kHotWaterStopped = 30,
};

inline HotWaterSwitchState currHotWaterSwitchState = kHotWaterSwitchIdle;
inline HotWaterState currHotWaterState = kHotWaterIdle;

inline uint8_t hotWaterSwitchReading = LOW;
inline uint8_t currReadingHotWaterSwitch = LOW;
inline double currPumpOnTime = 0;          // current running total pump on time
inline unsigned long pumpStartingTime = 0; // start time of pump

/**
 * @brief If set to publish debug messages then list what the current action is and what triggered it
 * @return void
 */
inline void debugHotWaterState(String state) {
}

/**
 * @brief True if in an active state, false if idle or just finished
 */
inline bool checkHotWaterStates() {
    return (currHotWaterState == kHotWaterRunning);
}

/**
 * @brief True if in a hot water state or in eith Steam with hot water on, false otherwise
 * if in an error state it will change away from these machine states and return false
 */
inline bool checkHotWaterActive() {
    return (g_state.machine.machineState == LegacyMachineState::kHotWater || (g_state.machine.machineState == LegacyMachineState::kSteam && checkHotWaterStates()));
}

/**
 * @brief True if in an active state, false if idle or just finished
 */
inline bool checkHotWaterStops() {

    if (g_state.machine.machineState == LegacyMachineState::kWaterTankEmpty) {
        debugHotWaterState("off-we"); // turn off due to water empty
        return true;
    }
    else if (g_state.machine.machineState == LegacyMachineState::kEmergencyStop || g_state.machine.machineState == LegacyMachineState::kSensorError) {
        debugHotWaterState("off-error"); // turn off due to error
        return true;
    }

    return false;
}

/**
 * @brief Toggle or momentary input for Hot Water Switch
 */
inline void checkHotWaterSwitch() {
    if (!isPowerSwitchOperationAllowed()) {
        return;
    }

    static bool loggedEmptyWaterTank = false;
    hotWaterSwitchReading = reinterpret_cast<Switch*>(g_state.hardware.hotWaterSwitch)->isPressed();

    // Block hotWaterSwitch input when water tank is empty
    if (g_state.machine.machineState == LegacyMachineState::kWaterTankEmpty) {

        if (!loggedEmptyWaterTank && (currHotWaterSwitchState == kHotWaterSwitchIdle || currHotWaterSwitchState == kHotWaterSwitchPressed)) {
            LOG(WARNING, "Hot water switch input ignored: Water tank empty");
            loggedEmptyWaterTank = true;
        }
        return;
    }

    loggedEmptyWaterTank = false;

    // Convert toggle hot water switch input to hot water switch state
    if (const int hotWaterSwitchType = static_cast<int>(Config::getInstance().hardwareSwitchesHotWaterType.get()); hotWaterSwitchType == Switch::TOGGLE) {
        if (currReadingHotWaterSwitch != hotWaterSwitchReading) {
            currReadingHotWaterSwitch = hotWaterSwitchReading;
        }

        switch (currHotWaterSwitchState) {
            case kHotWaterSwitchIdle:
                if (currReadingHotWaterSwitch == HIGH) {
                    currHotWaterSwitchState = kHotWaterSwitchShortPressed;
                    LOG(DEBUG, "Toggle Hot Water switch is ON -> got to currHotWaterSwitchState = kHotWaterSwitchShortPressed");
                }
                break;

            case kHotWaterSwitchShortPressed:
                if (currReadingHotWaterSwitch == LOW) {
                    currHotWaterSwitchState = kHotWaterSwitchIdle;
                    LOG(DEBUG, "Toggle Hot Water switch is OFF -> got to currHotWaterSwitchState = kHotWaterSwitchIdle");
                }
                else if (currHotWaterState == kHotWaterStopped) {
                    currHotWaterSwitchState = kHotWaterSwitchWaitForRelease;
                    LOG(DEBUG, "Hot Water has been Stopped -> got to currHotWaterSwitchState = kHotWaterSwitchWaitForRelease");
                }
                break;

            case kHotWaterSwitchWaitForRelease:
                if (currReadingHotWaterSwitch == LOW) {
                    currHotWaterSwitchState = kHotWaterSwitchIdle;
                    LOG(DEBUG, "Hot Water switch reset -> got to currHotWaterSwitchState = kHotWaterSwitchIdle");
                }
                break;

            default:

                currHotWaterSwitchState = kHotWaterSwitchIdle;
                LOG(DEBUG, "Unexpected switch state -> currHotWaterSwitchState = kHotWaterSwitchIdle");
                break;
        }
    }
    // Convert momentary hot water switch input to hot water switch state
    else if (hotWaterSwitchType == Switch::MOMENTARY) {
        if (currReadingHotWaterSwitch != hotWaterSwitchReading) {
            currReadingHotWaterSwitch = hotWaterSwitchReading;
        }

        switch (currHotWaterSwitchState) {
            case kHotWaterSwitchIdle:
                if (currReadingHotWaterSwitch == HIGH) {
                    currHotWaterSwitchState = kHotWaterSwitchPressed;
                    LOG(DEBUG, "Hot Water switch press detected -> got to currHotWaterSwitchState = kHotWaterSwitchPressed");
                }
                break;

            case kHotWaterSwitchPressed:                // Hot Water switch pressed - check for short or long press
                if (currReadingHotWaterSwitch == LOW) { // Hot Water switch short press detected
                    currHotWaterSwitchState = kHotWaterSwitchShortPressed;
                    LOG(DEBUG, "Hot Water switch short press detected -> got to currHotWaterSwitchState = kHotWaterSwitchShortPressed; start pump");
                }
                else if (currReadingHotWaterSwitch == HIGH && reinterpret_cast<Switch*>(g_state.hardware.hotWaterSwitch)->longPressDetected()) { // Hot Water switch long press detected
                    currHotWaterSwitchState = kHotWaterSwitchLongPressed;
                    LOG(DEBUG, "Hot Water switch long press detected -> got to currHotWaterSwitchState = kHotWaterSwitchLongPressed");
                }
                break;

            case kHotWaterSwitchShortPressed:
                if (currReadingHotWaterSwitch == HIGH) { // Hot Water switch short press detected while pump is running - stop pump
                    currHotWaterSwitchState = kHotWaterSwitchWaitForRelease;
                    LOG(DEBUG, "Hot Water switch short press detected -> got to currHotWaterSwitchState = kHotWaterSwitchWaitForRelease; pump stopped");
                }
                else if (currHotWaterState == kHotWaterStopped) { // pump stopped
                    currHotWaterSwitchState = kHotWaterSwitchWaitForRelease;
                    LOG(DEBUG, "Hot Water stopped -> got to currHotWaterSwitchState = kHotWaterSwitchWaitForRelease");
                }
                break;

            case kHotWaterSwitchLongPressed:
                if (currReadingHotWaterSwitch == LOW) { // Hot Water switch got released after long press detected - reset hot water switch
                    currHotWaterSwitchState = kHotWaterSwitchWaitForRelease;
                    LOG(DEBUG, "Hot Water switch long press released -> got to currHotWaterSwitchState = kHotWaterSwitchWaitForRelease");
                }
                break;

            case kHotWaterSwitchWaitForRelease: // wait for hot water switch got released
                if (currReadingHotWaterSwitch == LOW) {
                    currHotWaterSwitchState = kHotWaterSwitchIdle;
                    LOG(DEBUG, "Hot Water switch reset -> got to currHotWaterSwitchState = kHotWaterSwitchIdle");
                }
                break;

            default:
                currHotWaterSwitchState = kHotWaterSwitchIdle;
                LOG(DEBUG, "Unexpected switch state -> currHotWaterSwitchState = kHotWaterSwitchIdle");
                break;
        }
    }
}

/**
 * @brief Control the pump based on machine state
 * @return pumps state
 */
inline bool hotWaterHandler(void) {
    if (!Config::getInstance().hardwareSwitchesHotWaterEnabled.get() || g_state.hardware.hotWaterSwitch == nullptr) {
        return false; // hot water switch is not enabled
    }

    const unsigned long currentMillisTemp = millis();
    if (!checkBrewStates()) {
        checkHotWaterSwitch();
    }

    // abort function for state machine
    if (checkHotWaterStops()) {
        if (currHotWaterState != kHotWaterStopped) {
            LOG(INFO, "Pump Stopped");
        }
        currHotWaterState = kHotWaterStopped;
    }

    // calculated pump time while pump is running
    if (checkHotWaterStates()) {
        currPumpOnTime = currentMillisTemp - pumpStartingTime;
    }

    // state machine for hot water switch
    switch (currHotWaterState) {
        case kHotWaterIdle: // waiting step for hot water switch turning on

            if (currHotWaterSwitchState == kHotWaterSwitchShortPressed) {
                g_state.hardware.pumpRelay->on();
                pumpStartingTime = millis();
                currHotWaterState = kHotWaterRunning;
                currPumpOnTime = 0;          // reset currPumpOnTime
                LOG(INFO, "Hot water pump started");
                debugHotWaterState("on-sw"); // turned on due to switch input
            }

            break;

        case kHotWaterRunning:

            if (currHotWaterSwitchState == kHotWaterSwitchIdle && !checkBrewStates()) { // switch turned off and not in brew or flush
                currHotWaterState = kHotWaterStopped;
                debugHotWaterState("off-sw");
            }

            break;

        case kHotWaterStopped:
            g_state.hardware.pumpRelay->off();

            if (!checkHotWaterStops()) {
                currHotWaterState = kHotWaterIdle;
            }

            break;

        default:
            currHotWaterState = kHotWaterIdle;
            LOG(DEBUG, "Unexpected hot water state -> currHotWaterState = kHotWaterIdle");

            break;
    }

    return checkHotWaterStates();
}
