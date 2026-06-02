/**
 * @file test_main.cpp
 * @brief Brew-timer display FSM tests (UICoordinator-backed)
 */

#include <gtest/gtest.h>

#include "../ConfigTestHelper.h"
#include "../mocks/HandlerTestStubs.cpp"
#include "../mocks/MockWiFiManager.h"
#include "../test_support.h"

#include "clevercoffee/display/DisplayBrewTimerState.h"
#include "clevercoffee/handlers/BrewHandler.h"
#include "clevercoffee/state/MachineStateContext.h"

#include <memory>

using namespace CleverCoffee;
using ::testing::NiceMock;

class DisplayBrewTimerTest : public ::testing::Test {
  protected:
    void SetUp() override {
        systemContext_ = std::make_unique<SystemContext>();
        systemContext_->markReady();
        brewHandler_ = std::make_unique<BrewHandler>(*systemContext_, Config::getInstance());
        systemContext_->setBrewHandler(brewHandler_.get());

        dummyHwManager_      = std::make_unique<HardwareManager>(Config::getInstance());
        dummyDisplayManager_ = std::make_unique<DisplayManager>(::Hardware::OLEDType::SSD1306,
                                                                 ::Hardware::OLEDAddress::ADDR_3C);
        dummyMqttManager_    = std::make_unique<MQTTManager>();
        dummyWiFiManager_    = std::make_unique<NiceMock<MockWiFiManager>>();

        machineStateContext_ = std::make_unique<MachineStateContext>(
            *systemContext_, *dummyHwManager_, *dummyDisplayManager_, *dummyWiFiManager_, *dummyMqttManager_);
        systemContext_->setMachineStateContext(machineStateContext_.get());

        Config::getInstance().displayPostBrewTimerDuration.set(5.0);
    }

    void TearDown() override {
        systemContext_->setMachineStateContext(nullptr);
        machineStateContext_.reset();
        brewHandler_.reset();
        systemContext_.reset();
        resetConfigDefaults();
    }

    void setState(const MachineStateId state) {
        machineStateContext_->setCurrentStateId(state);
    }

    std::unique_ptr<SystemContext>             systemContext_;
    std::unique_ptr<BrewHandler>               brewHandler_;
    std::unique_ptr<MachineStateContext>       machineStateContext_;
    std::unique_ptr<HardwareManager>           dummyHwManager_;
    std::unique_ptr<DisplayManager>            dummyDisplayManager_;
    std::unique_ptr<MQTTManager>               dummyMqttManager_;
    std::unique_ptr<NiceMock<MockWiFiManager>> dummyWiFiManager_;
};

TEST_F(DisplayBrewTimerTest, IdleWhenNotBrewing) {
    setState(MachineStateId::PID_NORMAL);
    EXPECT_FALSE(shouldDisplayBrewTimer(*systemContext_));
    EXPECT_EQ(UICoordinator::BrewTimerDisplayState::Idle, systemContext_->uiCoordinator().getBrewTimerDisplayState());
}

TEST_F(DisplayBrewTimerTest, RunningWhenBrewActive) {
    setState(MachineStateId::BREW_RUNNING);
    EXPECT_TRUE(shouldDisplayBrewTimer(*systemContext_));
    EXPECT_EQ(UICoordinator::BrewTimerDisplayState::Running,
              systemContext_->uiCoordinator().getBrewTimerDisplayState());
}

TEST_F(DisplayBrewTimerTest, PostBrewAfterBrewEnds) {
    setState(MachineStateId::BREW_RUNNING);
    ASSERT_TRUE(shouldDisplayBrewTimer(*systemContext_));

    setState(MachineStateId::PID_NORMAL);
    EXPECT_TRUE(shouldDisplayBrewTimer(*systemContext_));
    EXPECT_EQ(UICoordinator::BrewTimerDisplayState::PostBrew,
              systemContext_->uiCoordinator().getBrewTimerDisplayState());
}

TEST_F(DisplayBrewTimerTest, IdleAfterPostBrewDuration) {
    systemContext_->uiCoordinator().setBrewTimerDisplayState(UICoordinator::BrewTimerDisplayState::PostBrew);
    systemContext_->uiCoordinator().setBrewTimerEndTime(millis() - 1000);
    Config::getInstance().displayPostBrewTimerDuration.set(0.0);

    EXPECT_FALSE(shouldDisplayBrewTimer(*systemContext_));
    EXPECT_EQ(UICoordinator::BrewTimerDisplayState::Idle, systemContext_->uiCoordinator().getBrewTimerDisplayState());
}
