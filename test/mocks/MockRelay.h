/**
 * @file test/mocks/MockRelay.h
 * @brief GMock implementation for Relay interface
 *
 * Allows testing code that depends on Relay behavior without actual hardware.
 */

#pragma once

#include <gmock/gmock.h>
#include "../../include/clevercoffee/hardware/Relay.h"

/**
 * @class MockRelay
 * @brief Mock implementation of Relay for testing
 *
 * Provides EXPECT_CALL() support for:
 * - on() - Turn relay on
 * - off() - Turn relay off
 * - getGPIOInstance() - Get the GPIO pin
 * - getTriggerType() - Get trigger type
 */
class MockRelay {
 public:
    /**
     * @brief Constructor with GPIO pin and trigger type
     */
    MockRelay(GPIOPin& gpio, Hardware::RelayTriggerType trigger = Hardware::RelayTriggerType::HIGH_TRIGGER)
        : gpio_(gpio), trigger_(trigger) {}

    /**
     * @brief Mock method to turn relay on
     */
    MOCK_METHOD(void, on, (), (const, noexcept));

    /**
     * @brief Mock method to turn relay off
     */
    MOCK_METHOD(void, off, (), (const, noexcept));

    /**
     * @brief Get GPIO instance (not mocked - returns real reference)
     */
    GPIOPin& getGPIOInstance() const noexcept {
        return gpio_;
    }

    /**
     * @brief Get trigger type (not mocked - returns stored value)
     */
    Hardware::RelayTriggerType getTriggerType() const noexcept {
        return trigger_;
    }

 private:
    GPIOPin& gpio_;
    Hardware::RelayTriggerType trigger_;
};
