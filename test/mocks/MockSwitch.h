/**
 * @file test/mocks/MockSwitch.h
 * @brief GMock implementation for Switch interface
 *
 * Allows testing code that depends on Switch behavior without actual hardware.
 */

#pragma once

#include <gmock/gmock.h>
#include "../../include/clevercoffee/hardware/Switch.h"

/**
 * @class MockSwitch
 * @brief Mock implementation of Switch abstract class for testing
 *
 * Provides EXPECT_CALL() support for:
 * - isPressed() - Check if switch is currently pressed
 * - longPressDetected() - Check if a long press was detected
 *
 * Example usage:
 * @code
 * MockSwitch mockSwitch(Hardware::SwitchType::BREW, Hardware::SwitchMode::MOMENTARY);
 * EXPECT_CALL(mockSwitch, isPressed()).WillOnce(::testing::Return(true));
 * EXPECT_CALL(mockSwitch, longPressDetected()).WillOnce(::testing::Return(false));
 * @endcode
 */
class MockSwitch : public Switch {
 public:
    /**
     * @brief Constructor with switch type and mode
     */
    MockSwitch(Hardware::SwitchType type, Hardware::SwitchMode mode)
        : Switch(type, mode) {}

    /**
     * @brief Mock method to check if switch is pressed
     */
    MOCK_METHOD(bool, isPressed, (), (override));

    /**
     * @brief Mock method to check for long press detection
     */
    MOCK_METHOD(bool, longPressDetected, (), (override));
};
