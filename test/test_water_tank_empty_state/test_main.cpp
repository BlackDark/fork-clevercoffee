/**
 * @file test_main.cpp
 * @brief Unit tests for WaterTankEmptyState standby transitions
 *
 * Safety requirement: the heater must never run unattended forever. When the
 * machine sits in WATER_TANK_EMPTY (especially with
 * hardware.sensors.watertank.keep_heater_on_empty=true), the standby request
 * and standby timeout must still move the machine to STANDBY, which turns the
 * heater off. Conversely, a machine already resting in STANDBY must not be
 * woken up just because the tank is (still) empty.
 */

#include <gmock/gmock.h>
#include <gtest/gtest.h>

#include <memory>

#include "../ConfigTestHelper.h"
#include "../mocks/HandlerTestStubs.cpp"
#include "../mocks/MockSwitch.h"
#include "../mocks/MockWiFiManager.h"
#include "../test_support.h"

#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/state/states/ErrorStates.h"
#include "clevercoffee/state/states/SystemStates.h"

const char* getStateName(MachineStateId) {
    return "test";
}

#include "../../src/state/states/ErrorStates.cpp"
#include "../../src/state/states/SystemStates.cpp"

using ::testing::NiceMock;
using ::testing::Return;

// ============================================================================
// TEST FIXTURE
// ============================================================================

class WaterTankEmptyStateTest : public ::testing::Test {
  protected:
    void SetUp() override {
        g_test_millis = 0;
        Preferences::resetTestStore();
        CleverCoffee::TestHardwareSpy::reset();
        (void)Config::getInstance().hardwareSensorsWatertankEnabled.set(true);

        systemContext_ = std::make_unique<CleverCoffee::SystemContext>();
        systemContext_->markReady();

        hwManager_      = std::make_unique<CleverCoffee::HardwareManager>(Config::getInstance());
        displayManager_ = std::make_unique<DisplayManager>(::Hardware::OLEDType::SSD1306,
                                                           ::Hardware::OLEDAddress::ADDR_3C);
        mqttManager_    = std::make_unique<MQTTManager>();
        wifiManager_    = std::make_unique<NiceMock<MockWiFiManager>>();

        context_ = std::make_unique<MachineStateContext>(
            *systemContext_, *hwManager_, *displayManager_, *wifiManager_, *mqttManager_);
        systemContext_->setMachineStateContext(context_.get());

        context_->updateStateEntryTime(std::chrono::steady_clock::now());
    }

    void TearDown() override {
        systemContext_->setMachineStateContext(nullptr);
        context_.reset();
        hwManager_.reset();
        displayManager_.reset();
        mqttManager_.reset();
        wifiManager_.reset();
        systemContext_.reset();
        (void)Config::getInstance().hardwareSensorsWatertankEnabled.set(false);
        g_test_millis = 0;
    }

    /// Inject a mock tank sensor and drive SensorCoordinator until it reports empty.
    void makeTankEmpty() {
        auto mock =
            std::make_unique<NiceMock<MockSwitch>>(::Hardware::SwitchType::TOGGLE, ::Hardware::SwitchMode::NORMALLY_OPEN);
        tankSensor_ = mock.get();
        ON_CALL(*tankSensor_, isPressed()).WillByDefault(Return(false));
        systemContext_->sensorCoordinator().setWaterTankSensor(std::move(mock));

        systemContext_->sensorCoordinator().update();
        g_test_millis += 250;
        systemContext_->sensorCoordinator().update();
        ASSERT_FALSE(systemContext_->sensorCoordinator().isWaterTankFull());
    }

    /// Flip the injected sensor back to "water detected" and re-read.
    void refillTank() {
        ON_CALL(*tankSensor_, isPressed()).WillByDefault(Return(true));
        g_test_millis += 250;
        systemContext_->sensorCoordinator().update();
        ASSERT_TRUE(systemContext_->sensorCoordinator().isWaterTankFull());
    }

    std::unique_ptr<CleverCoffee::SystemContext>   systemContext_;
    std::unique_ptr<CleverCoffee::HardwareManager> hwManager_;
    std::unique_ptr<DisplayManager>                displayManager_;
    std::unique_ptr<MQTTManager>                   mqttManager_;
    std::unique_ptr<NiceMock<MockWiFiManager>>     wifiManager_;
    std::unique_ptr<MachineStateContext>           context_;
    NiceMock<MockSwitch>*                          tankSensor_ = nullptr;
};

// ============================================================================
// WATER_TANK_EMPTY → STANDBY (heater must not run unattended forever)
// ============================================================================

TEST_F(WaterTankEmptyStateTest, StandbyRequestTransitionsToStandby) {
    makeTankEmpty();
    context_->setStandbyRequested(true);

    WaterTankEmptyState state;
    auto                next = state.checkTransitions(*context_);

    ASSERT_TRUE(next.has_value()) << "Standby request must be honored while tank is empty";
    EXPECT_EQ(MachineStateId::STANDBY, next.value());
    EXPECT_FALSE(context_->isStandbyRequested()) << "Request flag must be drained";
}

TEST_F(WaterTankEmptyStateTest, StandbyTimeoutTransitionsToStandby) {
    makeTankEmpty();
    CleverCoffee::TestHardwareSpy::shouldEnterStandby = true;

    WaterTankEmptyState state;
    auto                next = state.checkTransitions(*context_);

    ASSERT_TRUE(next.has_value()) << "Standby timeout must fire while tank is empty";
    EXPECT_EQ(MachineStateId::STANDBY, next.value());
}

TEST_F(WaterTankEmptyStateTest, StaysPutWithoutStandbySignalOrRefill) {
    makeTankEmpty();

    WaterTankEmptyState state;
    auto                next = state.checkTransitions(*context_);

    EXPECT_FALSE(next.has_value()) << "No signal, no transition (self-transitions are noise)";
}

TEST_F(WaterTankEmptyStateTest, RefillReturnsToPidState) {
    makeTankEmpty();
    refillTank();

    WaterTankEmptyState state;
    auto                next = state.checkTransitions(*context_);

    ASSERT_TRUE(next.has_value());
    EXPECT_EQ(context_->getPidState(), next.value());
}

// ============================================================================
// STANDBY must not be interrupted by an empty tank
// ============================================================================

TEST_F(WaterTankEmptyStateTest, StandbyStateIsNotForcedOutByEmptyTank) {
    makeTankEmpty();

    StandbyState standby;
    auto         next = standby.checkTransitions(*context_);

    EXPECT_FALSE(next.has_value()) << "Standby (heater off) must persist while the tank is empty";
}
