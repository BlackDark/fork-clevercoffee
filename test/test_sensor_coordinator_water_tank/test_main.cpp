/**
 * @file test_main.cpp
 * @brief Unit tests for SensorCoordinator water tank ownership and readings
 */

#include <gmock/gmock.h>
#include <gtest/gtest.h>

#include "../mocks/ConfigStubs.cpp"
#include "../mocks/MockSwitch.h"
#include "../test_support.h"
#include "clevercoffee/Config.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"

#include "../../src/Logger.cpp"
#include "../../src/coordinators/SensorCoordinator.cpp"

using namespace CleverCoffee;
using ::testing::Return;

class SensorCoordinatorWaterTankTest : public ::testing::Test {
protected:
    void SetUp() override {
        g_test_millis = 0;
        Config::getInstance().hardwareSensorsWatertankEnabled.set(true);
    }

    void TearDown() override {
        Config::getInstance().hardwareSensorsWatertankEnabled.set(false);
    }
};

TEST_F(SensorCoordinatorWaterTankTest, OwnsInjectedSensor) {
    auto        mock = std::make_unique<MockSwitch>(Hardware::SwitchType::TOGGLE, Hardware::SwitchMode::NORMALLY_OPEN);
    const Switch* raw = mock.get();

    SensorCoordinator coord;
    coord.setWaterTankSensor(std::move(mock));

    EXPECT_EQ(raw, coord.getWaterTankSensor());
}

TEST_F(SensorCoordinatorWaterTankTest, DefaultsToFullBeforeFirstRead) {
    SensorCoordinator coord;
    EXPECT_TRUE(coord.isWaterTankFull());
}

TEST_F(SensorCoordinatorWaterTankTest, DetectsTransitionToEmpty) {
    auto mock     = std::make_unique<MockSwitch>(Hardware::SwitchType::TOGGLE, Hardware::SwitchMode::NORMALLY_OPEN);
    auto* mockPtr = mock.get();
    EXPECT_CALL(*mockPtr, isPressed()).WillRepeatedly(Return(false));

    SensorCoordinator coord;
    coord.setWaterTankSensor(std::move(mock));

    coord.update();
    g_test_millis = 250;
    coord.update();

    EXPECT_FALSE(coord.isWaterTankFull());
}

TEST_F(SensorCoordinatorWaterTankTest, DetectsRefillAfterEmpty) {
    auto mock     = std::make_unique<MockSwitch>(Hardware::SwitchType::TOGGLE, Hardware::SwitchMode::NORMALLY_OPEN);
    auto* mockPtr = mock.get();

    SensorCoordinator coord;
    coord.setWaterTankSensor(std::move(mock));

    EXPECT_CALL(*mockPtr, isPressed()).WillRepeatedly(Return(false));
    coord.update();
    g_test_millis = 250;
    coord.update();
    ASSERT_FALSE(coord.isWaterTankFull());

    EXPECT_CALL(*mockPtr, isPressed()).WillRepeatedly(Return(true));
    g_test_millis = 500;
    coord.update();

    EXPECT_TRUE(coord.isWaterTankFull());
}

TEST_F(SensorCoordinatorWaterTankTest, IgnoresSensorWhenDisabled) {
    Config::getInstance().hardwareSensorsWatertankEnabled.set(false);

    auto mock     = std::make_unique<MockSwitch>(Hardware::SwitchType::TOGGLE, Hardware::SwitchMode::NORMALLY_OPEN);
    auto* mockPtr = mock.get();
    EXPECT_CALL(*mockPtr, isPressed()).Times(0);

    SensorCoordinator coord;
    coord.setWaterTankSensor(std::move(mock));

    g_test_millis = 250;
    coord.update();

    EXPECT_TRUE(coord.isWaterTankFull());
}

TEST_F(SensorCoordinatorWaterTankTest, ClearingOwnedSensorStopsReads) {
    auto mock     = std::make_unique<MockSwitch>(Hardware::SwitchType::TOGGLE, Hardware::SwitchMode::NORMALLY_OPEN);
    auto* mockPtr = mock.get();
    EXPECT_CALL(*mockPtr, isPressed()).Times(0);

    SensorCoordinator coord;
    coord.setWaterTankSensor(std::move(mock));
    coord.setWaterTankSensor(nullptr);

    g_test_millis = 250;
    coord.update();

    EXPECT_EQ(nullptr, coord.getWaterTankSensor());
    EXPECT_TRUE(coord.isWaterTankFull());
}

TEST_F(SensorCoordinatorWaterTankTest, SystemContextUsesSameCoordinatorState) {
    auto  mock    = std::make_unique<MockSwitch>(Hardware::SwitchType::TOGGLE, Hardware::SwitchMode::NORMALLY_OPEN);
    auto* mockPtr = mock.get();
    SystemContext context;
    context.sensorCoordinator().setWaterTankSensor(std::move(mock));

    EXPECT_CALL(*mockPtr, isPressed()).WillRepeatedly(Return(false));
    context.sensorCoordinator().update();
    g_test_millis = 250;
    context.sensorCoordinator().update();

    EXPECT_FALSE(context.sensorCoordinator().isWaterTankFull());
}
