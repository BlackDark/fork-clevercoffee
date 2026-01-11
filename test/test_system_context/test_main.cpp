/**
 * @file test_main.cpp
 * @brief Unit tests for SystemContext
 */

#include <gtest/gtest.h>
#include "../test_support.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include "clevercoffee/coordinators/NetworkCoordinator.h"
#include "clevercoffee/coordinators/UICoordinator.h"
#include "clevercoffee/coordinators/StandbyCoordinator.h"
#include "clevercoffee/context/HardwareContext.h"
#include <memory>

using namespace CleverCoffee;

class SystemContextTest : public ::testing::Test {
protected:
    void SetUp() override {
        systemContext_ = std::make_unique<SystemContext>();
    }

    void TearDown() override {
        systemContext_.reset();
    }

    std::unique_ptr<SystemContext> systemContext_;
};

TEST_F(SystemContextTest, InitiallyNotReady) {
    EXPECT_FALSE(systemContext_->isReady());
}

TEST_F(SystemContextTest, CanMarkReady) {
    systemContext_->markReady();
    EXPECT_TRUE(systemContext_->isReady());
}

TEST_F(SystemContextTest, HasSensorCoordinator) {
    // SensorCoordinator should be accessible
    auto& coord = systemContext_->sensorCoordinator();
    EXPECT_DOUBLE_EQ(0.0, coord.getTemperature());
    EXPECT_DOUBLE_EQ(0.0, coord.getWeight());
}

TEST_F(SystemContextTest, HasNetworkCoordinator) {
    // NetworkCoordinator should be accessible
    auto& coord = systemContext_->networkCoordinator();
    // Basic check that coordinator exists
    SUCCEED();
}

TEST_F(SystemContextTest, HasUICoordinator) {
    // UICoordinator should be accessible
    auto& coord = systemContext_->uiCoordinator();
    // Basic check that coordinator exists
    SUCCEED();
}

TEST_F(SystemContextTest, HasStandbyCoordinator) {
    // StandbyCoordinator should be accessible
    auto& coord = systemContext_->standbyCoordinator();
    // Basic check that coordinator exists
    SUCCEED();
}

TEST_F(SystemContextTest, HasHardwareContext) {
    // HardwareContext should be accessible
    auto& hw = systemContext_->hardwareContext();
    // Basic check that context exists
    SUCCEED();
}

TEST_F(SystemContextTest, ProcessTemperatureDefaultsToZero) {
    EXPECT_DOUBLE_EQ(0.0, systemContext_->processTemperature());
}

TEST_F(SystemContextTest, ProcessPidOutputDefaultsToZero) {
    EXPECT_EQ(0, systemContext_->processPidOutput());
}

TEST_F(SystemContextTest, CanSetAndGetProcessTemperature) {
    systemContext_->setProcessTemperature(95.5);
    EXPECT_DOUBLE_EQ(95.5, systemContext_->processTemperature());
}

TEST_F(SystemContextTest, CanSetAndGetProcessPidOutput) {
    systemContext_->setProcessPidOutput(500);
    EXPECT_EQ(500, systemContext_->processPidOutput());
}

TEST_F(SystemContextTest, ProcessSetpointDefaultsTo95) {
    EXPECT_DOUBLE_EQ(95.0, systemContext_->processSetpoint());
}

TEST_F(SystemContextTest, CanSetAndGetProcessSetpoint) {
    systemContext_->setProcessSetpoint(98.5);
    EXPECT_DOUBLE_EQ(98.5, systemContext_->processSetpoint());
}

TEST_F(SystemContextTest, ProcessCurrentBrewTimeDefaultsToZero) {
    EXPECT_DOUBLE_EQ(0.0, systemContext_->processCurrentBrewTime());
}

TEST_F(SystemContextTest, CanSetAndGetProcessCurrentBrewTime) {
    systemContext_->setProcessCurrentBrewTime(5000.0);
    EXPECT_DOUBLE_EQ(5000.0, systemContext_->processCurrentBrewTime());
}

TEST_F(SystemContextTest, ProcessBrewPidDisabledDefaultsToFalse) {
    EXPECT_FALSE(systemContext_->isProcessBrewPidDisabled());
}

TEST_F(SystemContextTest, CanSetAndGetProcessBrewPidDisabled) {
    systemContext_->setProcessBrewPidDisabled(true);
    EXPECT_TRUE(systemContext_->isProcessBrewPidDisabled());
    systemContext_->setProcessBrewPidDisabled(false);
    EXPECT_FALSE(systemContext_->isProcessBrewPidDisabled());
}

TEST_F(SystemContextTest, CanSetAndGetCurrBrewWeight) {
    systemContext_->setCurrBrewWeight(25.5);
    EXPECT_DOUBLE_EQ(25.5, systemContext_->currBrewWeight());
}

TEST_F(SystemContextTest, CanSetAndGetCurrReadingWeight) {
    systemContext_->setCurrReadingWeight(30.0);
    EXPECT_DOUBLE_EQ(30.0, systemContext_->currReadingWeight());
}

TEST_F(SystemContextTest, CanSetAndGetPreBrewWeight) {
    systemContext_->setPreBrewWeight(5.0f);
    EXPECT_FLOAT_EQ(5.0f, systemContext_->preBrewWeight());
}
