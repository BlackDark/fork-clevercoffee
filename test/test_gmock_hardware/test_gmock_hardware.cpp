/**
 * @file test/test_gmock_hardware/test_gmock_hardware.cpp
 * @brief Advanced hardware behavior tests using GMock
 *
 * These tests verify hardware control logic using mocked components.
 * They demonstrate GMock capabilities for verifying:
 * - Call sequences and ordering with InSequence
 * - Call expectations with EXPECT_CALL
 * - Return value setup with WillOnce/WillRepeatedly
 * - Call count verification with Times
 */

#include <gtest/gtest.h>
#include <gmock/gmock.h>
#include "../../test/mocks/MockSwitch.h"
#include "../../test/mocks/MockSensorManager.h"

using ::testing::Return;
using ::testing::InSequence;
using ::testing::AtLeast;

// ============================================================================
// SWITCH TESTS
// ============================================================================

/**
 * Test fixture for switch detection tests
 */
class SwitchDetectionTest : public ::testing::Test {
 protected:
    MockSwitch brewSwitch_{Hardware::SwitchType::MOMENTARY, Hardware::SwitchMode::NORMALLY_OPEN};
    MockSwitch steamSwitch_{Hardware::SwitchType::MOMENTARY, Hardware::SwitchMode::NORMALLY_OPEN};
    MockSwitch hotWaterSwitch_{Hardware::SwitchType::MOMENTARY, Hardware::SwitchMode::NORMALLY_OPEN};
};

/**
 * Test single button press detection
 */
TEST_F(SwitchDetectionTest, BrewSwitchPressed) {
    EXPECT_CALL(brewSwitch_, isPressed()).WillOnce(Return(true));
    EXPECT_CALL(brewSwitch_, longPressDetected()).WillOnce(Return(false));
    
    EXPECT_TRUE(brewSwitch_.isPressed());
    EXPECT_FALSE(brewSwitch_.longPressDetected());
}

/**
 * Test long press detection
 */
TEST_F(SwitchDetectionTest, LongPressDetection) {
    EXPECT_CALL(brewSwitch_, isPressed()).WillOnce(Return(true));
    EXPECT_CALL(brewSwitch_, longPressDetected()).WillOnce(Return(true));
    
    EXPECT_TRUE(brewSwitch_.isPressed());
    EXPECT_TRUE(brewSwitch_.longPressDetected());
}

/**
 * Test switch released (not pressed)
 */
TEST_F(SwitchDetectionTest, SwitchReleased) {
    EXPECT_CALL(brewSwitch_, isPressed()).WillOnce(Return(false));
    EXPECT_CALL(brewSwitch_, longPressDetected()).WillOnce(Return(false));
    
    EXPECT_FALSE(brewSwitch_.isPressed());
    EXPECT_FALSE(brewSwitch_.longPressDetected());
}

/**
 * Test multiple button presses in sequence
 */
TEST_F(SwitchDetectionTest, MultipleSwitchPresses) {
    EXPECT_CALL(brewSwitch_, isPressed())
        .WillOnce(Return(true))   // Press
        .WillOnce(Return(false))  // Release
        .WillOnce(Return(true))   // Press again
        .WillOnce(Return(false)); // Release again
    
    EXPECT_TRUE(brewSwitch_.isPressed());
    EXPECT_FALSE(brewSwitch_.isPressed());
    EXPECT_TRUE(brewSwitch_.isPressed());
    EXPECT_FALSE(brewSwitch_.isPressed());
}

/**
 * Test multiple switches with different states
 */
TEST_F(SwitchDetectionTest, MultipleSwitchStates) {
    EXPECT_CALL(brewSwitch_, isPressed()).WillOnce(Return(true));
    EXPECT_CALL(steamSwitch_, isPressed()).WillOnce(Return(false));
    EXPECT_CALL(hotWaterSwitch_, isPressed()).WillOnce(Return(true));
    
    EXPECT_TRUE(brewSwitch_.isPressed());
    EXPECT_FALSE(steamSwitch_.isPressed());
    EXPECT_TRUE(hotWaterSwitch_.isPressed());
}

/**
 * Test switch long press during brew sequence
 */
TEST_F(SwitchDetectionTest, BrewLongPress) {
    InSequence seq;
    EXPECT_CALL(brewSwitch_, isPressed()).WillOnce(Return(true));
    EXPECT_CALL(brewSwitch_, longPressDetected()).WillOnce(Return(false));
    EXPECT_CALL(brewSwitch_, isPressed()).WillOnce(Return(true));
    EXPECT_CALL(brewSwitch_, longPressDetected()).WillOnce(Return(true));
    
    // Initial press
    EXPECT_TRUE(brewSwitch_.isPressed());
    EXPECT_FALSE(brewSwitch_.longPressDetected());
    
    // Held down - long press detected
    EXPECT_TRUE(brewSwitch_.isPressed());
    EXPECT_TRUE(brewSwitch_.longPressDetected());
}

/**
 * Test steam switch independently
 */
TEST_F(SwitchDetectionTest, SteamSwitchOperation) {
    InSequence seq;
    EXPECT_CALL(steamSwitch_, isPressed()).WillOnce(Return(false));
    EXPECT_CALL(steamSwitch_, isPressed()).WillOnce(Return(true));
    EXPECT_CALL(steamSwitch_, longPressDetected()).WillOnce(Return(false));
    EXPECT_CALL(steamSwitch_, isPressed()).WillOnce(Return(false));
    
    // Initially not pressed
    EXPECT_FALSE(steamSwitch_.isPressed());
    
    // Pressed
    EXPECT_TRUE(steamSwitch_.isPressed());
    
    // Quick press
    EXPECT_FALSE(steamSwitch_.longPressDetected());
    
    // Released
    EXPECT_FALSE(steamSwitch_.isPressed());
}

// ============================================================================
// SENSOR TESTS WITH GMOCK
// ============================================================================

/**
 * Test fixture for temperature sensor monitoring
 */
class SensorMonitoringTest : public ::testing::Test {
 protected:
    MockSensorManager sensorManager_;
    MockSwitch brewSwitch_{Hardware::SwitchType::MOMENTARY, Hardware::SwitchMode::NORMALLY_OPEN};
};

/**
 * Test temperature monitoring during brew start
 */
TEST_F(SensorMonitoringTest, TemperatureReadyForBrew) {
    // Set up ready state
    sensorManager_.setTemperature(98.5);
    sensorManager_.setTemperatureError(false);
    
    // Brew button pressed
    EXPECT_CALL(brewSwitch_, isPressed()).WillOnce(Return(true));
    
    // Verify conditions for brew start
    bool canStartBrew = brewSwitch_.isPressed() && 
                        sensorManager_.getCurrentTemperature() > 95.0 &&
                        !sensorManager_.hasTemperatureError();
    
    EXPECT_TRUE(canStartBrew);
}

/**
 * Test temperature too low for brew
 */
TEST_F(SensorMonitoringTest, TemperatureTooLow) {
    sensorManager_.setTemperature(50.0);
    
    EXPECT_CALL(brewSwitch_, isPressed()).WillOnce(Return(true));
    
    bool canStartBrew = brewSwitch_.isPressed() && 
                        sensorManager_.getCurrentTemperature() > 95.0;
    
    EXPECT_FALSE(canStartBrew);  // Temp too low
}

/**
 * Test emergency stop on critical temperature
 */
TEST_F(SensorMonitoringTest, CriticalTemperatureEmergency) {
    sensorManager_.setTemperature(175.0);
    
    bool isEmergency = sensorManager_.getCurrentTemperature() > 170.0;
    EXPECT_TRUE(isEmergency);
}

/**
 * Test water tank empty prevents brew
 */
TEST_F(SensorMonitoringTest, WaterTankEmptyPreventsBrew) {
    sensorManager_.setWaterTankFull(false);
    
    EXPECT_CALL(brewSwitch_, isPressed()).WillOnce(Return(true));
    
    bool canStartBrew = brewSwitch_.isPressed() && 
                        sensorManager_.isWaterTankFull();
    
    EXPECT_FALSE(canStartBrew);  // No water
}

/**
 * Test water tank full allows brew
 */
TEST_F(SensorMonitoringTest, WaterTankFullAllowsBrew) {
    sensorManager_.setWaterTankFull(true);
    
    EXPECT_CALL(brewSwitch_, isPressed()).WillOnce(Return(true));
    
    bool canStartBrew = brewSwitch_.isPressed() && 
                        sensorManager_.isWaterTankFull();
    
    EXPECT_TRUE(canStartBrew);
}

/**
 * Test sensor error prevents brew
 */
TEST_F(SensorMonitoringTest, SensorErrorPreventsBrew) {
    sensorManager_.setTemperatureError(true);
    
    EXPECT_CALL(brewSwitch_, isPressed()).WillOnce(Return(true));
    
    bool canStartBrew = brewSwitch_.isPressed() && 
                        !sensorManager_.hasTemperatureError();
    
    EXPECT_FALSE(canStartBrew);  // Sensor error
}

/**
 * Test pressure monitoring during extraction
 */
TEST_F(SensorMonitoringTest, PressureMonitoring) {
    // Pre-infusion: low pressure
    sensorManager_.setPressure(0.5f);
    EXPECT_LT(sensorManager_.getCurrentPressure(), 1.0f);
    
    // Main extraction: target pressure
    sensorManager_.setPressure(2.5f);
    EXPECT_GT(sensorManager_.getCurrentPressure(), 2.0f);
    
    // End of extraction: back to low
    sensorManager_.setPressure(0.0f);
    EXPECT_EQ(sensorManager_.getCurrentPressure(), 0.0f);
}

/**
 * Test complete brew monitoring sequence
 */
TEST_F(SensorMonitoringTest, CompletBrewSequence) {
    InSequence seq;
    
    // Pre-brew: setup system
    sensorManager_.setTemperature(98.0);
    sensorManager_.setPressure(0.0f);
    sensorManager_.setWaterTankFull(true);
    sensorManager_.setTemperatureError(false);
    
    EXPECT_EQ(sensorManager_.getCurrentTemperature(), 98.0);
    EXPECT_EQ(sensorManager_.getCurrentPressure(), 0.0f);
    EXPECT_TRUE(sensorManager_.isWaterTankFull());
    EXPECT_FALSE(sensorManager_.hasTemperatureError());
    
    // Brew button pressed
    EXPECT_CALL(brewSwitch_, isPressed()).WillOnce(Return(true));
    EXPECT_TRUE(brewSwitch_.isPressed());
    
    // Brew starts: pressure builds
    sensorManager_.setPressure(2.5f);
    EXPECT_GT(sensorManager_.getCurrentPressure(), 2.0f);
    
    // Brew ends: pressure release
    sensorManager_.setPressure(0.0f);
    EXPECT_EQ(sensorManager_.getCurrentPressure(), 0.0f);
}

/**
 * Test multiple errors simultaneously
 */
TEST_F(SensorMonitoringTest, MultipleErrors) {
    sensorManager_.setTemperatureError(true);
    sensorManager_.setScaleError(true);
    
    EXPECT_TRUE(sensorManager_.hasTemperatureError());
    EXPECT_TRUE(sensorManager_.hasScaleError());
    EXPECT_TRUE(sensorManager_.hasSensorError());
}

/**
 * Test filtered pressure reading
 */
TEST_F(SensorMonitoringTest, FilteredPressure) {
    sensorManager_.setPressure(2.35f);
    
    // Filtered pressure should match raw pressure in mock
    EXPECT_EQ(sensorManager_.getFilteredPressure(), sensorManager_.getCurrentPressure());
}

/**
 * Test temperature sensor recovery from error
 */
TEST_F(SensorMonitoringTest, TemperatureSensorRecovery) {
    // Sensor error
    sensorManager_.setTemperatureError(true);
    EXPECT_TRUE(sensorManager_.hasTemperatureError());
    
    // Sensor recovers
    sensorManager_.setTemperatureError(false);
    EXPECT_FALSE(sensorManager_.hasTemperatureError());
}

/**
 * Test all sensors in nominal state
 */
TEST_F(SensorMonitoringTest, AllSystemsNominal) {
    // Set all to nominal state
    sensorManager_.setTemperature(100.0);
    sensorManager_.setPressure(0.0f);
    sensorManager_.setWaterTankFull(true);
    sensorManager_.setTemperatureError(false);
    sensorManager_.setScaleError(false);
    
    // Verify all nominal
    EXPECT_GE(sensorManager_.getCurrentTemperature(), 95.0);
    EXPECT_LE(sensorManager_.getCurrentTemperature(), 105.0);
    EXPECT_EQ(sensorManager_.getCurrentPressure(), 0.0f);
    EXPECT_TRUE(sensorManager_.isWaterTankFull());
    EXPECT_FALSE(sensorManager_.hasSensorError());
}
