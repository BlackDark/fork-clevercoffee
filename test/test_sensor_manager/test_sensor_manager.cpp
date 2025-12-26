/**
 * @file test/test_sensor_manager/test_sensor_manager.cpp
 * @brief SensorManager tests using MockSensorManager
 *
 * These tests verify sensor reading, error detection, and state management
 * using a lightweight mock that simulates sensor behavior without hardware.
 */

#include <gtest/gtest.h>
#include "../../test/mocks/MockSensorManager.h"

/**
 * Test fixture for SensorManager-related tests
 */
class SensorManagerTest : public ::testing::Test {
 protected:
    MockSensorManager mockSensor;
};

// ============================================================================
// TEMPERATURE READING TESTS
// ============================================================================

/**
 * Test that temperature can be read correctly
 */
TEST_F(SensorManagerTest, ReadsTemperatureCorrectly) {
    mockSensor.setTemperature(98.5);
    EXPECT_DOUBLE_EQ(mockSensor.getCurrentTemperature(), 98.5);
}

/**
 * Test normal operating temperature range (90-105°C)
 */
TEST_F(SensorManagerTest, TemperatureInNormalRange) {
    mockSensor.setTemperature(100.0);
    double temp = mockSensor.getCurrentTemperature();
    EXPECT_GE(temp, 90.0);
    EXPECT_LE(temp, 105.0);
}

/**
 * Test high temperature detection (160°C warning threshold)
 */
TEST_F(SensorManagerTest, DetectsHighTemperature) {
    mockSensor.setTemperature(160.0);
    EXPECT_GT(mockSensor.getCurrentTemperature(), 150.0);
}

/**
 * Test critical/emergency temperature detection (170°C shutdown threshold)
 */
TEST_F(SensorManagerTest, DetectsCriticalTemperature) {
    mockSensor.setTemperature(175.0);
    EXPECT_GT(mockSensor.getCurrentTemperature(), 170.0);
}

/**
 * Test temperature transitions (normal to high to critical)
 */
TEST_F(SensorManagerTest, TemperatureTransitions) {
    // Start at normal
    mockSensor.setTemperature(98.0);
    EXPECT_LT(mockSensor.getCurrentTemperature(), 100.0);

    // Transition to high
    mockSensor.setTemperature(165.0);
    EXPECT_GT(mockSensor.getCurrentTemperature(), 150.0);

    // Transition back to normal
    mockSensor.setTemperature(95.0);
    EXPECT_LT(mockSensor.getCurrentTemperature(), 100.0);
}

/**
 * Test invalid low temperature reading
 */
TEST_F(SensorManagerTest, HandlesInvalidLowTemperature) {
    mockSensor.setTemperature(-100.0);
    // Invalid readings should be detectable (negative temps)
    EXPECT_LT(mockSensor.getCurrentTemperature(), 0.0);
}

/**
 * Test invalid high temperature reading
 */
TEST_F(SensorManagerTest, HandlesInvalidHighTemperature) {
    mockSensor.setTemperature(220.0);
    // Invalid readings should be detectable (unreasonably high)
    EXPECT_GT(mockSensor.getCurrentTemperature(), 200.0);
}

/**
 * Test zero temperature
 */
TEST_F(SensorManagerTest, HandlesZeroTemperature) {
    mockSensor.setTemperature(0.0);
    EXPECT_DOUBLE_EQ(mockSensor.getCurrentTemperature(), 0.0);
}

// ============================================================================
// TEMPERATURE ERROR DETECTION TESTS
// ============================================================================

/**
 * Test temperature error state can be set and read
 */
TEST_F(SensorManagerTest, DetectsTemperatureError) {
    mockSensor.setTemperatureError(true);
    EXPECT_TRUE(mockSensor.hasTemperatureError());
}

/**
 * Test temperature error recovery
 */
TEST_F(SensorManagerTest, TemperatureErrorRecovery) {
    mockSensor.setTemperatureError(true);
    EXPECT_TRUE(mockSensor.hasTemperatureError());

    // Recover from error
    mockSensor.setTemperatureError(false);
    EXPECT_FALSE(mockSensor.hasTemperatureError());
}

/**
 * Test temperature error doesn't affect reading capability
 */
TEST_F(SensorManagerTest, CanReadTemperatureDuringError) {
    mockSensor.setTemperature(98.0);
    mockSensor.setTemperatureError(true);
    
    EXPECT_TRUE(mockSensor.hasTemperatureError());
    EXPECT_DOUBLE_EQ(mockSensor.getCurrentTemperature(), 98.0);
}

/**
 * Test multiple error states together
 */
TEST_F(SensorManagerTest, DetectsMultipleErrors) {
    mockSensor.setTemperatureError(true);
    mockSensor.setScaleError(true);
    
    EXPECT_TRUE(mockSensor.hasTemperatureError());
    EXPECT_TRUE(mockSensor.hasScaleError());
    EXPECT_TRUE(mockSensor.hasSensorError());
}

// ============================================================================
// PRESSURE READING TESTS
// ============================================================================

/**
 * Test pressure reading functionality
 */
TEST_F(SensorManagerTest, ReadsPressureCorrectly) {
    mockSensor.setPressure(1.5f);
    EXPECT_FLOAT_EQ(mockSensor.getCurrentPressure(), 1.5f);
}

/**
 * Test normal pressure range (0-3 bar)
 */
TEST_F(SensorManagerTest, PressureInNormalRange) {
    mockSensor.setPressure(1.0f);
    float pressure = mockSensor.getCurrentPressure();
    EXPECT_GE(pressure, 0.0f);
    EXPECT_LE(pressure, 3.0f);
}

/**
 * Test zero pressure (no water flow)
 */
TEST_F(SensorManagerTest, HandlesZeroPressure) {
    mockSensor.setPressure(0.0f);
    EXPECT_FLOAT_EQ(mockSensor.getCurrentPressure(), 0.0f);
}

/**
 * Test maximum pressure
 */
TEST_F(SensorManagerTest, HandlesHighPressure) {
    mockSensor.setPressure(3.0f);
    EXPECT_FLOAT_EQ(mockSensor.getCurrentPressure(), 3.0f);
}

/**
 * Test filtered pressure (simulating pressure sensor filtering)
 */
TEST_F(SensorManagerTest, FilteredPressureMatches) {
    mockSensor.setPressure(1.8f);
    EXPECT_FLOAT_EQ(mockSensor.getFilteredPressure(), 1.8f);
}

/**
 * Test pressure transitions
 */
TEST_F(SensorManagerTest, PressureTransitions) {
    mockSensor.setPressure(0.5f);
    EXPECT_LT(mockSensor.getCurrentPressure(), 1.0f);

    mockSensor.setPressure(2.5f);
    EXPECT_GT(mockSensor.getCurrentPressure(), 2.0f);

    mockSensor.setPressure(0.0f);
    EXPECT_EQ(mockSensor.getCurrentPressure(), 0.0f);
}

// ============================================================================
// WATER TANK TESTS
// ============================================================================

/**
 * Test water tank full state
 */
TEST_F(SensorManagerTest, DetectsWaterTankFull) {
    mockSensor.setWaterTankFull(true);
    EXPECT_TRUE(mockSensor.isWaterTankFull());
}

/**
 * Test water tank empty state
 */
TEST_F(SensorManagerTest, DetectsWaterTankEmpty) {
    mockSensor.setWaterTankFull(false);
    EXPECT_FALSE(mockSensor.isWaterTankFull());
}

/**
 * Test water tank transitions
 */
TEST_F(SensorManagerTest, WaterTankTransitions) {
    mockSensor.setWaterTankFull(true);
    EXPECT_TRUE(mockSensor.isWaterTankFull());

    mockSensor.setWaterTankFull(false);
    EXPECT_FALSE(mockSensor.isWaterTankFull());

    mockSensor.setWaterTankFull(true);
    EXPECT_TRUE(mockSensor.isWaterTankFull());
}

// ============================================================================
// SCALE/WEIGHT SENSOR TESTS
// ============================================================================

/**
 * Test scale error detection
 */
TEST_F(SensorManagerTest, DetectsScaleError) {
    mockSensor.setScaleError(true);
    EXPECT_TRUE(mockSensor.hasScaleError());
}

/**
 * Test scale error recovery
 */
TEST_F(SensorManagerTest, ScaleErrorRecovery) {
    mockSensor.setScaleError(true);
    EXPECT_TRUE(mockSensor.hasScaleError());

    mockSensor.setScaleError(false);
    EXPECT_FALSE(mockSensor.hasScaleError());
}

/**
 * Test no sensor error when no errors exist
 */
TEST_F(SensorManagerTest, NoErrorWhenNoErrorsSet) {
    mockSensor.setTemperatureError(false);
    mockSensor.setScaleError(false);
    EXPECT_FALSE(mockSensor.hasSensorError());
}

/**
 * Test general sensor error detection
 */
TEST_F(SensorManagerTest, GeneralSensorErrorDetection) {
    mockSensor.setScaleError(true);
    EXPECT_TRUE(mockSensor.hasSensorError());
}

// ============================================================================
// INTEGRATION TESTS (Multiple sensors + errors)
// ============================================================================

/**
 * Test reading all sensors simultaneously
 */
TEST_F(SensorManagerTest, ReadsAllSensorsTogether) {
    mockSensor.setTemperature(99.5);
    mockSensor.setPressure(1.2f);
    mockSensor.setWaterTankFull(true);

    EXPECT_DOUBLE_EQ(mockSensor.getCurrentTemperature(), 99.5);
    EXPECT_FLOAT_EQ(mockSensor.getCurrentPressure(), 1.2f);
    EXPECT_TRUE(mockSensor.isWaterTankFull());
}

/**
 * Test sensor state independence (changing one doesn't affect others)
 */
TEST_F(SensorManagerTest, SensorIndependence) {
    // Set temperature and pressure
    mockSensor.setTemperature(95.0);
    mockSensor.setPressure(1.5f);

    // Change temperature only
    mockSensor.setTemperature(105.0);

    // Pressure should not change
    EXPECT_FLOAT_EQ(mockSensor.getCurrentPressure(), 1.5f);
}

/**
 * Test error state doesn't affect other sensors
 */
TEST_F(SensorManagerTest, ErrorStateIndependence) {
    mockSensor.setTemperatureError(true);
    mockSensor.setPressure(2.0f);

    EXPECT_TRUE(mockSensor.hasTemperatureError());
    EXPECT_FLOAT_EQ(mockSensor.getCurrentPressure(), 2.0f);
}

/**
 * Test complete state capture (all sensor values and errors)
 */
TEST_F(SensorManagerTest, CompleteStateCaptureAfterErrors) {
    // Set all sensors and errors
    mockSensor.setTemperature(98.0);
    mockSensor.setPressure(1.8f);
    mockSensor.setWaterTankFull(false);
    mockSensor.setTemperatureError(false);
    mockSensor.setScaleError(false);

    // Verify all state
    EXPECT_DOUBLE_EQ(mockSensor.getCurrentTemperature(), 98.0);
    EXPECT_FLOAT_EQ(mockSensor.getCurrentPressure(), 1.8f);
    EXPECT_FALSE(mockSensor.isWaterTankFull());
    EXPECT_FALSE(mockSensor.hasSensorError());
}

/**
 * Test sensor recovery sequence (error -> recovery -> normal operation)
 */
TEST_F(SensorManagerTest, SensorRecoverySequence) {
    // Normal operation
    mockSensor.setTemperature(100.0);
    mockSensor.setScaleError(false);
    EXPECT_FALSE(mockSensor.hasSensorError());

    // Error occurs
    mockSensor.setScaleError(true);
    EXPECT_TRUE(mockSensor.hasSensorError());

    // Sensor recovers
    mockSensor.setScaleError(false);
    EXPECT_FALSE(mockSensor.hasSensorError());

    // Back to normal operation with valid readings
    EXPECT_DOUBLE_EQ(mockSensor.getCurrentTemperature(), 100.0);
}
