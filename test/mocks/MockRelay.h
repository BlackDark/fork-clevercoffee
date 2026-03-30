/**
 * @file test/mocks/MockRelay.h
 * @brief GMock implementation for Relay interface
 *
 * Standalone mock that does NOT depend on GPIOPin or real hardware.
 * Mirrors the Relay public API so it can be used as a drop-in replacement
 * in tests via templates or dependency injection.
 */

#pragma once

#include <gmock/gmock.h>
#include "clevercoffee/defaults.h"

/**
 * @class MockRelay
 * @brief Mock implementation of Relay for testing
 *
 * Does not require GPIOPin. Trigger type is configurable at construction.
 *
 * Provides EXPECT_CALL() support for:
 * - on()  — Turn relay on
 * - off() — Turn relay off
 *
 * getTriggerType() returns the stored value (not mocked).
 */
class MockRelay {
  public:
    /**
     * @brief Default constructor (HIGH_TRIGGER)
     */
    explicit MockRelay(Hardware::RelayTriggerType trigger = Hardware::RelayTriggerType::HIGH_TRIGGER)
        : trigger_(trigger) {}

    /**
     * @brief Mock method to turn relay on
     */
    MOCK_METHOD(void, on, (), (const, noexcept));

    /**
     * @brief Mock method to turn relay off
     */
    MOCK_METHOD(void, off, (), (const, noexcept));

    /**
     * @brief Get trigger type (not mocked — returns stored value)
     */
    Hardware::RelayTriggerType getTriggerType() const noexcept {
        return trigger_;
    }

  private:
    Hardware::RelayTriggerType trigger_;
};
