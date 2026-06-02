/**
 * @file test_main.cpp
 * @brief Preinfusion pause keeps valve open and pump off
 */

#include <gtest/gtest.h>

#include <memory>

#include "../ConfigTestHelper.h"
#include "../test_support.h"
#include "../mocks/MockWiFiManager.h"
#include "../mocks/HandlerTestStubs.cpp"

#include "clevercoffee/hardware/HardwareManager.h"
#include "clevercoffee/state/states/BrewStates.h"

const char* getStateName(MachineStateId) {
    return "test";
}

#include "../../src/state/states/BrewStates.cpp"

using ::testing::NiceMock;

class BrewPreinfusionPauseTest : public ::testing::Test {
  protected:
    void SetUp() override {
        CleverCoffee::TestHardwareSpy::reset();
        systemContext_ = std::make_unique<CleverCoffee::SystemContext>();
        systemContext_->markReady();

        dummyHwManager_      = std::make_unique<CleverCoffee::HardwareManager>(Config::getInstance());
        dummyDisplayManager_ = std::make_unique<DisplayManager>(::Hardware::OLEDType::SSD1306,
                                                                ::Hardware::OLEDAddress::ADDR_3C);
        dummyMqttManager_    = std::make_unique<MQTTManager>();
        dummyWiFiManager_    = std::make_unique<NiceMock<MockWiFiManager>>();

        machineStateContext_ = std::make_unique<MachineStateContext>(
            *systemContext_, *dummyHwManager_, *dummyDisplayManager_, *dummyWiFiManager_, *dummyMqttManager_);
        systemContext_->setMachineStateContext(machineStateContext_.get());
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

    std::unique_ptr<CleverCoffee::SystemContext>  systemContext_;
    std::unique_ptr<MachineStateContext>          machineStateContext_;
    std::unique_ptr<CleverCoffee::HardwareManager> dummyHwManager_;
    std::unique_ptr<DisplayManager>               dummyDisplayManager_;
    std::unique_ptr<MQTTManager>                  dummyMqttManager_;
    std::unique_ptr<NiceMock<MockWiFiManager>>    dummyWiFiManager_;
};

TEST_F(BrewPreinfusionPauseTest, PauseEntryStopsPumpAndOpensValve) {
    BrewPreinfusionPauseState state;
    state.onEntry(*machineStateContext_);

    EXPECT_GE(CleverCoffee::TestHardwareSpy::disablePumpCalls, 1);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::openWaterValveCalls, 1);
    EXPECT_EQ(CleverCoffee::TestHardwareSpy::enablePumpCalls, 0);
}

TEST_F(BrewPreinfusionPauseTest, PauseUpdateKeepsPumpOffAndValveOpen) {
    BrewPreinfusionPauseState state;
    state.update(*machineStateContext_);

    EXPECT_GE(CleverCoffee::TestHardwareSpy::disablePumpCalls, 1);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::openWaterValveCalls, 1);
    EXPECT_EQ(CleverCoffee::TestHardwareSpy::enablePumpCalls, 0);
}
