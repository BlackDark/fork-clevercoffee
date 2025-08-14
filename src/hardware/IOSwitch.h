/**
 * @file GPIOPin.h
 *
 * @brief A physical switch connected to a GPIO Pin
 */

#pragma once

#include <cstdint>
#include <chrono>
#include "GPIOPin.h"
#include "defaults.h"
#include "Switch.h"

class IOSwitch final : public Switch {
    public:
        /**
         * @brief I/O Switch connected to a GPIO pin
         * @details This class represents a switch which is connected to a GPIO pin of the controller. It holds its own
         *          instance of the GPIO pin.
         *
         * @param pinNumber GPIO pin number
         * @param pinType GPIO pin type
         * @param switchType Type of the switch
         * @param mode Operation mode of the switch
         * @param initialState
         */
        IOSwitch(int pinNumber, GPIOPin::Type pinType, Hardware::SwitchType switchType = Hardware::SwitchType::MOMENTARY, Hardware::SwitchMode mode = Hardware::SwitchMode::NORMALLY_OPEN, uint8_t initialState = LOW);

        /**
         * @brief Switch reading (pressed, not pressed)
         * @details This function reads the connected GPIO pin and returns a debounced reading of the witch
         * @return True of activated, false otherwise
         */
        bool isPressed() override;

        /**
         * @brief Check if a long press of the momentary switch has been detected
         * @details Always returns false for toggle switches
         * @return True if the momentary switch is held down, false otherwise
         */
        bool longPressDetected() override;

    private:
        // Pin to operate on:
        GPIOPin gpio;

        // Switch state
        uint8_t lastState;
        uint8_t currentState;

        // Debouncing and long-press detection with std::chrono
        std::chrono::steady_clock::time_point lastStateChangeTime{};
        std::chrono::steady_clock::time_point pressStartTime{};
        std::chrono::steady_clock::time_point lastDebounceTime{};
        bool longPressTriggered{false};

        static constexpr std::chrono::milliseconds debounceDelay{20};
        static constexpr std::chrono::milliseconds longPressDuration{500};
};
