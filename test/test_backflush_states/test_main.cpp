/**
 * @file test_main.cpp
 * @brief Unit tests for backflush state transitions and maintenance reset
 */

#include <gtest/gtest.h>
#include <gmock/gmock.h>

#include <chrono>
#include <memory>

#include "../ConfigTestHelper.h"
#include "../test_support.h"
#include "../mocks/MockWiFiManager.h"
#include "../mocks/HandlerTestStubs.cpp"

#include "clevercoffee/coordinators/MaintenanceCoordinator.h"
#include "clevercoffee/defaults.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/state/states/BackflushStates.h"

const char* getStateName(MachineStateId) {
    return "test";
}

#include "../../src/state/states/BackflushStates.cpp"

using CleverCoffee::MaintenanceCoordinator;
using ::testing::NiceMock;
using namespace std::chrono_literals;

class BackflushStatesTest : public ::testing::Test {
  protected:
    void SetUp() override {
        Preferences::resetTestStore();
        systemContext_ = std::make_unique<CleverCoffee::SystemContext>();
        systemContext_->markReady();
        ASSERT_TRUE(systemContext_->maintenanceCoordinator().begin());

        dummyHwManager_      = std::make_unique<CleverCoffee::HardwareManager>(Config::getInstance());
        dummyDisplayManager_ = std::make_unique<DisplayManager>(::Hardware::OLEDType::SSD1306,
                                                                ::Hardware::OLEDAddress::ADDR_3C);
        dummyMqttManager_    = std::make_unique<MQTTManager>();
        dummyWiFiManager_    = std::make_unique<NiceMock<MockWiFiManager>>();

        machineStateContext_ = std::make_unique<MachineStateContext>(
            *systemContext_, *dummyHwManager_, *dummyDisplayManager_, *dummyWiFiManager_, *dummyMqttManager_);
        systemContext_->setMachineStateContext(machineStateContext_.get());

        ASSERT_TRUE(Config::getInstance().pidEnabled.set(true));
        ASSERT_TRUE(Config::getInstance().backflushCycles.set(5));
        ASSERT_TRUE(Config::getInstance().backflushFillTime.set(BACKFLUSH_FILL_TIME));
        ASSERT_TRUE(Config::getInstance().backflushFlushTime.set(BACKFLUSH_FLUSH_TIME));

        machineStateContext_->updateStateEntryTime(std::chrono::steady_clock::now());
    }

    void TearDown() override {
        systemContext_->setMachineStateContext(nullptr);
        machineStateContext_.reset();
        dummyWiFiManager_.reset();
        dummyMqttManager_.reset();
        dummyDisplayManager_.reset();
        dummyHwManager_.reset();
        systemContext_.reset();
        resetConfigDefaults();
    }

    void expireStateTimeout() {
        machineStateContext_->updateStateEntryTime(std::chrono::steady_clock::now() - 10s);
    }

    std::unique_ptr<CleverCoffee::SystemContext>  systemContext_;
    std::unique_ptr<MachineStateContext>          machineStateContext_;
    std::unique_ptr<CleverCoffee::HardwareManager> dummyHwManager_;
    std::unique_ptr<DisplayManager>               dummyDisplayManager_;
    std::unique_ptr<MQTTManager>                  dummyMqttManager_;
    std::unique_ptr<NiceMock<MockWiFiManager>>    dummyWiFiManager_;
};

TEST_F(BackflushStatesTest, DisableClearsCycleStartRequestFlag) {
    ASSERT_TRUE(machineStateContext_->applyBackflushMode(true));
    machineStateContext_->setBackflushCycleStartRequested(true);

    ASSERT_TRUE(machineStateContext_->applyBackflushMode(false));

    EXPECT_FALSE(machineStateContext_->isBackflushCycleStartRequested());
    EXPECT_FALSE(machineStateContext_->isBackflushEnterRequested());
}

TEST_F(BackflushStatesTest, ModeDisabledMidFillTransitionsToPid) {
    ASSERT_TRUE(Config::getInstance().backflushFillTime.set(BACKFLUSH_FILL_TIME));
    ASSERT_TRUE(machineStateContext_->applyBackflushMode(true));
    machineStateContext_->setCurrentStateId(MachineStateId::BACKFLUSH_FILLING);
    machineStateContext_->updateStateEntryTime(std::chrono::steady_clock::now());
    ASSERT_TRUE(machineStateContext_->applyBackflushMode(false));
    EXPECT_FALSE(machineStateContext_->isBackflushModeActive());

    BackflushFillingState state;
    const auto            next = state.checkTransitions(*machineStateContext_);

    ASSERT_TRUE(next.has_value());
    EXPECT_EQ(machineStateContext_->getPidState(), *next);
}

TEST_F(BackflushStatesTest, LastCycleTransitionsToFinished) {
    ASSERT_TRUE(machineStateContext_->applyBackflushMode(true));
    machineStateContext_->setCurrentStateId(MachineStateId::BACKFLUSH_FLUSHING);
    machineStateContext_->setBackflushCycleCount(5);
    expireStateTimeout();

    BackflushFlushingState state;
    const auto             next = state.checkTransitions(*machineStateContext_);

    ASSERT_TRUE(next.has_value());
    EXPECT_EQ(MachineStateId::BACKFLUSH_FINISHED, *next);
}

TEST_F(BackflushStatesTest, FinishedOnEntryResetsMaintenanceCounter) {
    systemContext_->maintenanceCoordinator().recordBrewIfQualified(
        BACKFLUSH_REMINDER_MIN_BREW_TIME_MS, 0.0f, false);
    ASSERT_GT(systemContext_->maintenanceCoordinator().getShotsSinceBackflush(), 0);

    BackflushFinishedState state;
    state.onEntry(*machineStateContext_);

    EXPECT_EQ(0, systemContext_->maintenanceCoordinator().getShotsSinceBackflush());
}
