/**
 * @file IOSwitch.h
 *
 * @brief A physical switch connected to a GPIO Pin
 */

#include "IOSwitch.h"
#include "GPIOPin.h"

#include "Logger.h"
IOSwitch::IOSwitch(const int pinNumber, const GPIOPin::Type pinType, const Type switchType, const Mode mode, const uint8_t initialState) :
    Switch(switchType, mode), gpio(pinNumber, pinType), lastState(initialState), currentState(LOW) {
}

bool IOSwitch::isPressed() {
    const uint8_t reading = gpio.read();
    const auto currentTime = std::chrono::steady_clock::now();

    if (reading != lastState) {
        lastDebounceTime = currentTime;
    }

    if (currentTime - lastDebounceTime > debounceDelay) {
        if ((reading ^ mode_) != currentState) {
            currentState = reading ^ mode_;

            if (currentState == LOW) {
                lastStateChangeTime = currentTime;
            }
            else {
                pressStartTime = currentTime;
            }
        }
    }

    lastState = reading;

    if (type_ == MOMENTARY) {
        if (currentState == HIGH && (currentTime - pressStartTime) >= longPressDuration) {
            longPressTriggered = true;
        }
        else if (currentState == LOW && lastStateChangeTime == currentTime) {
            longPressTriggered = false;
        }
    }

    return currentState == HIGH;
}

bool IOSwitch::longPressDetected() {
    if (type_ == TOGGLE) {
        return false;
    }

    if (type_ == MOMENTARY) {
        return longPressTriggered;
    }

    return false;
}
