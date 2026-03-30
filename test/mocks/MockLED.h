/**
 * @file test/mocks/MockLED.h
 * @brief GMock implementation for LED interface
 *
 * Allows testing code that depends on LED behavior without actual hardware.
 */

#pragma once

#include <gmock/gmock.h>
#include "../../include/clevercoffee/hardware/LED.h"

/**
 * @class MockLED
 * @brief Mock implementation of LED abstract class for testing
 *
 * Provides EXPECT_CALL() support for:
 * - turnOn() - Turn LED on
 * - turnOff() - Turn LED off
 * - setColor() - Set LED color (RGB)
 * - setBrightness() - Set LED brightness level
 * - setGPIOState() - Set GPIO state
 *
 * Example usage:
 * @code
 * MockLED mockLED;
 * EXPECT_CALL(mockLED, turnOn()).Times(1);
 * EXPECT_CALL(mockLED, setColor(255, 0, 0)).Times(1);  // Red color
 * @endcode
 */
class MockLED : public LED {
 public:
    /**
     * @brief Mock method to turn LED on
     */
    MOCK_METHOD(void, turnOn, (), (override));

    /**
     * @brief Mock method to turn LED off
     */
    MOCK_METHOD(void, turnOff, (), (override));

    /**
     * @brief Mock method to set LED color (RGB)
     *
     * @param red Red component (0-255)
     * @param green Green component (0-255)
     * @param blue Blue component (0-255)
     */
    MOCK_METHOD(void, setColor, (int red, int green, int blue), (override));

    /**
     * @brief Mock method to set LED brightness
     *
     * @param value Brightness level (0-255)
     */
    MOCK_METHOD(void, setBrightness, (int value), (override));

    /**
     * @brief Mock method to set GPIO state
     *
     * @param state GPIO state (true/false)
     */
    MOCK_METHOD(void, setGPIOState, (bool state), (override));
};
