/**
 * @file MockDisplayManager.h
 * @brief Mock implementation of DisplayManager for testing
 *
 * Allows testing code that depends on DisplayManager without actual display hardware.
 */

#pragma once

#include <gmock/gmock.h>
#include "../../include/clevercoffee/display/DisplayManager.h"
#include <U8g2lib.h>

/**
 * @class MockDisplayManager
 * @brief Mock implementation of DisplayManager for testing
 *
 * Provides EXPECT_CALL() support for display operations.
 * 
 * Example usage:
 * @code
 * MockDisplayManager mockDisplay;
 * EXPECT_CALL(mockDisplay, isInitialized()).WillRepeatedly(Return(true));
 * EXPECT_CALL(mockDisplay, getDisplay()).WillRepeatedly(Return(nullptr));
 * @endcode
 */
class MockDisplayManager {
public:
    MockDisplayManager() = default;
    virtual ~MockDisplayManager() = default;

    /**
     * @brief Mock method to check if display is initialized
     */
    MOCK_METHOD(bool, isInitialized, (), (const, noexcept));

    /**
     * @brief Mock method to get U8G2 display pointer
     */
    MOCK_METHOD(U8G2*, getDisplay, (), (const, noexcept));
};
