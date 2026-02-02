/**
 * @file MockDisplayManager.h
 * @brief Mock implementation of IDisplayManager for testing
 */

#pragma once

#include <gmock/gmock.h>
#include "clevercoffee/display/IDisplayManager.h"

// Forward declaration for U8G2 stub
class U8G2;

/**
 * @class MockDisplayManager
 * @brief Google Mock implementation of IDisplayManager interface
 *
 * Provides a test double for DisplayManager that can be used with
 * GoogleTest expectations and behavior specifications.
 */
class MockDisplayManager : public IDisplayManager {
  public:
    MockDisplayManager()          = default;
    ~MockDisplayManager() override = default;

    MOCK_METHOD(U8G2*, getDisplay, (), (const, noexcept, override));
    MOCK_METHOD(bool, isInitialized, (), (const, noexcept, override));
};

/**
 * @brief Create a MockDisplayManager with default behavior for common scenarios
 * @return Unique pointer to NiceMock<MockDisplayManager> with sensible defaults
 *
 * Default behavior:
 * - getDisplay() returns nullptr
 * - isInitialized() returns true
 */
inline std::unique_ptr<testing::NiceMock<MockDisplayManager>> createDefaultMockDisplayManager() {
    auto mock = std::make_unique<testing::NiceMock<MockDisplayManager>>();
    ON_CALL(*mock, getDisplay()).WillByDefault(testing::Return(nullptr));
    ON_CALL(*mock, isInitialized()).WillByDefault(testing::Return(true));
    return mock;
}
