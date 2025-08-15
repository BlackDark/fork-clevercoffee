/**
 * @file StandardLED.h
 *
 * @brief An LED connected to a GPIO pin
 */

#pragma once

#include "clevercoffee/hardware/LED.h"

class GPIOPin;

class StandardLED final : public LED {
    public:
        StandardLED(GPIOPin& gpioInstance, bool inverted);

        void turnOn() override;
        void turnOff() override;
        void setColor(int red, int green, int blue) override;
        void setBrightness(int value) override;
        void setGPIOState(bool state) override;
        
        /**
         * @brief Check if LED is inverted
         * @return true if inverted logic
         */
        [[nodiscard]] constexpr bool isInverted() const noexcept { return inverted; }
        
        /**
         * @brief Check if LED is enabled
         * @return true if enabled
         */
        [[nodiscard]] constexpr bool isEnabled() const noexcept { return enabled; }
        
        /**
         * @brief Enable or disable the LED
         * @param enable true to enable, false to disable
         */
        void setEnabled(bool enable) noexcept { enabled = enable; }

    private:
        GPIOPin& gpio;
        bool inverted;  // If true, invert on/off behavior
        bool enabled{}; // If false, the LED will be disabled
};
