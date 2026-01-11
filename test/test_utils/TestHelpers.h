/**
 * @file TestHelpers.h
 * @brief Test utilities and helpers for unit testing
 * 
 * Provides common test utilities, fixtures, and helpers to make testing easier.
 */

#pragma once

#include <gtest/gtest.h>
#include "../mocks/MockConfig.h"
#include "../mocks/MockSensorManager.h"
#include "../mocks/MockRelay.h"
#include "../mocks/MockSwitch.h"
#include "../mocks/MockLED.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include "clevercoffee/sensors/ISensor.h"
#include "clevercoffee/hardware/GPIOPin.h"
#include <memory>

namespace CleverCoffee {

/**
 * @brief Test fixture base class with common setup
 * 
 * Provides common test infrastructure:
 * - SystemContext instance
 * - Mock sensors
 * - Test configuration
 */
class TestFixtureBase : public ::testing::Test {
protected:
    void SetUp() override {
        systemContext_ = std::make_unique<SystemContext>();
        // Additional setup can be done in derived classes
    }

    void TearDown() override {
        systemContext_.reset();
    }

    std::unique_ptr<SystemContext> systemContext_;
    MockConfig mockConfig_;
    MockSensorManager mockSensorManager_;
};

/**
 * @brief Helper to create a test SystemContext with minimal setup
 * 
 * @return Unique pointer to SystemContext ready for testing
 */
inline std::unique_ptr<SystemContext> createTestSystemContext() {
    auto ctx = std::make_unique<SystemContext>();
    ctx->markReady();
    return ctx;
}

// Forward declaration - MockSensor is defined in test files
// This allows test files to define their own MockSensor implementations

/**
 * @brief Test configuration builder for easy test setup
 * 
 * Usage:
 * @code
 * TestConfigBuilder builder;
 * builder.setPidEnabled(true)
 *        .setBrewSetpoint(95.0)
 *        .setSteamSetpoint(120.0);
 * MockConfig config = builder.build();
 * @endcode
 */
class TestConfigBuilder {
public:
    TestConfigBuilder() = default;

    TestConfigBuilder& setPidEnabled(bool enabled) {
        config_.setPidEnabled(enabled);
        return *this;
    }

    TestConfigBuilder& setBrewSetpoint(double setpoint) {
        config_.setBrewSetpoint(setpoint);
        return *this;
    }

    TestConfigBuilder& setSteamSetpoint(double setpoint) {
        config_.setSteamSetpoint(setpoint);
        return *this;
    }

    TestConfigBuilder& setStandbyEnabled(bool enabled) {
        config_.setStandbyEnabled(enabled);
        return *this;
    }

    TestConfigBuilder& setStandbyTime(double minutes) {
        config_.setStandbyTime(minutes);
        return *this;
    }

    TestConfigBuilder& setBrewSwitchEnabled(bool enabled) {
        config_.setHardwareSwitchesBrewEnabled(enabled);
        return *this;
    }

    TestConfigBuilder& setBrewSwitchType(Hardware::SwitchType type) {
        config_.setHardwareSwitchesBrewType(type);
        return *this;
    }

    MockConfig build() const {
        return config_;
    }

private:
    MockConfig config_;
};

} // namespace CleverCoffee
