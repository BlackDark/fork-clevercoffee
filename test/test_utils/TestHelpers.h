/**
 * @file TestHelpers.h
 * @brief Test utilities and helpers for unit testing
 *
 * Provides common test utilities, fixtures, and helpers to make testing easier.
 */

#pragma once

#include <gtest/gtest.h>

#include "../ConfigTestHelper.h"
#include "../mocks/MockConfig.h"
#include "../mocks/MockLED.h"
#include "../mocks/MockRelay.h"
#include "../mocks/MockSensorManager.h"
#include "../mocks/MockSwitch.h"

#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include "clevercoffee/hardware/GPIOPin.h"
#include "clevercoffee/sensors/ISensor.h"

#include <memory>

namespace CleverCoffee {

/**
 * @brief Test fixture base class with common setup
 *
 * Provides common test infrastructure:
 * - SystemContext instance
 * - Mock sensors
 * - Mock configuration (implements IConfig)
 *
 * Automatically resets the Config singleton to defaults in TearDown
 * for tests that still access Config::getInstance() directly.
 */
class TestFixtureBase : public ::testing::Test {
  protected:
    void SetUp() override {
        systemContext_ = std::make_unique<SystemContext>();
    }

    void TearDown() override {
        systemContext_.reset();
        resetConfigDefaults();
    }

    std::unique_ptr<SystemContext> systemContext_;
    MockConfig                    mockConfig_;
    MockSensorManager             mockSensorManager_;
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

    TestConfigBuilder& setBrewMode(Process::BrewMode mode) {
        config_.setBrewMode(mode);
        return *this;
    }

    TestConfigBuilder& setBrewByTimeEnabled(bool enabled) {
        config_.setBrewByTimeEnabled(enabled);
        return *this;
    }

    TestConfigBuilder& setBrewByTimeTargetTime(double time) {
        config_.setBrewByTimeTargetTime(time);
        return *this;
    }

    TestConfigBuilder& setPreInfusionEnabled(bool enabled) {
        config_.setBrewPreInfusionEnabled(enabled);
        return *this;
    }

    TestConfigBuilder& setPreInfusionTime(double time) {
        config_.setBrewPreInfusionTime(time);
        return *this;
    }

    TestConfigBuilder& setPreInfusionPause(double pause) {
        config_.setBrewPreInfusionPause(pause);
        return *this;
    }

    TestConfigBuilder& setEmergencyStopTemp(double temp) {
        config_.setEmergencyStopTemp(temp);
        return *this;
    }

    TestConfigBuilder& setEmergencyStopHysteresis(double hysteresis) {
        config_.setEmergencyStopHysteresis(hysteresis);
        return *this;
    }

    MockConfig build() const { return config_; }

  private:
    MockConfig config_;
};

/**
 * @brief Test fixture for ProcessController tests
 *
 * Provides common setup for ProcessController testing:
 * - SystemContext with PID controller
 * - MachineStateContext
 * - Config singleton reset in TearDown
 */
class ProcessControllerTestFixture : public TestFixtureBase {
  protected:
    void SetUp() override { TestFixtureBase::SetUp(); }
};

/**
 * @brief Test fixture for StateMachine tests
 *
 * Provides common setup for StateMachine testing:
 * - SystemContext
 * - MachineStateContext
 * - Config singleton reset in TearDown
 */
class StateMachineTestFixture : public TestFixtureBase {
  protected:
    void SetUp() override { TestFixtureBase::SetUp(); }
};

/**
 * @brief Test fixture for handler tests
 *
 * Provides common setup for handler testing:
 * - SystemContext
 * - MockConfig (IConfig-based)
 * - Config singleton reset in TearDown
 */
class HandlerTestFixture : public TestFixtureBase {
  protected:
    void SetUp() override { TestFixtureBase::SetUp(); }
};

/**
 * @brief Test fixture for integration tests
 *
 * Provides full system setup for integration testing
 */
class IntegrationTestFixture : public TestFixtureBase {
  protected:
    void SetUp() override { TestFixtureBase::SetUp(); }
};

} // namespace CleverCoffee
