#include <gtest/gtest.h>
#include "../test_support.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include "clevercoffee/sensors/ISensor.h"
#include "../mocks/MockISensor.h"
#include <thread>
#include <chrono>
#include <memory>

using namespace CleverCoffee;
using MockSensor = MockISensor;  // Use shared mock implementation

TEST(SensorCoordinatorTest, InitiallyNotRunning) {
    SensorCoordinator coord;
    // Sensor updates are now automatic and non-blocking
    // No need to check if updates are running - they always run automatically
    // This test verifies the coordinator can be created without errors
    EXPECT_DOUBLE_EQ(0.0, coord.getTemperature());
    EXPECT_DOUBLE_EQ(0.0, coord.getWeight());
}

TEST(SensorCoordinatorTest, CanReadTemperature) {
    auto mockTemp = std::make_unique<MockSensor>(95.5);
    SensorCoordinator coord(mockTemp.get(), nullptr, nullptr);
    
    // Update multiple times to allow async read to complete
    coord.update();
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    coord.update();
    
    EXPECT_DOUBLE_EQ(95.5, coord.getTemperature());
}

TEST(SensorCoordinatorTest, CanReadWeight) {
    auto mockScale = std::make_unique<MockSensor>(250.0);
    SensorCoordinator coord(nullptr, mockScale.get(), nullptr);
    
    // Update multiple times to allow async read to complete
    coord.update();
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    coord.update();
    
    EXPECT_DOUBLE_EQ(250.0, coord.getWeight());
}

TEST(SensorCoordinatorTest, HandlesNullSensors) {
    SensorCoordinator coord(nullptr, nullptr, nullptr);
    
    // Should not crash
    coord.update();
    EXPECT_DOUBLE_EQ(0.0, coord.getTemperature());
    EXPECT_DOUBLE_EQ(0.0, coord.getWeight());
}

TEST(SensorCoordinatorTest, FiltersPressureCorrectly) {
    SensorCoordinator coord;
    
    // Pressure filtering is internal - test via getFilteredPressure
    // First update with pressure reading
    coord.update();
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    coord.update();
    
    // Get filtered pressure (should be initialized to 0.0)
    float filtered1 = coord.getFilteredPressure();
    EXPECT_GE(filtered1, 0.0f);
    
    // After more updates, filtered value should stabilize
    coord.update();
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    coord.update();
    
    float filtered2 = coord.getFilteredPressure();
    EXPECT_GE(filtered2, 0.0f);
}

TEST(SensorCoordinatorTest, HandlesSensorErrors) {
    auto mockTemp = std::make_unique<MockSensor>();
    mockTemp->setShouldFail(true);  // Simulate sensor error
    
    SensorCoordinator coord(mockTemp.get(), nullptr, nullptr);
    
    // Update should handle error gracefully
    coord.update();
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    coord.update();
    
    // Should report error
    EXPECT_TRUE(coord.hasTemperatureSensorError());
    EXPECT_TRUE(coord.hasSensorError());
}

TEST(SensorCoordinatorTest, BrewWeightTracking) {
    auto mockScale = std::make_unique<MockSensor>(0.0);
    SensorCoordinator coord(nullptr, mockScale.get(), nullptr);
    
    // Start with zero weight
    coord.update();
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    coord.update();
    
    EXPECT_FALSE(coord.isBrewWeightTrackingActive());
    EXPECT_DOUBLE_EQ(0.0, coord.getBrewWeight());
    
    // Start tracking
    coord.startBrewWeightTracking();
    EXPECT_TRUE(coord.isBrewWeightTrackingActive());
    EXPECT_DOUBLE_EQ(0.0, coord.getPreBrewWeight());
    
    // Simulate weight increase
    mockScale->setValue(50.0);
    coord.update();
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    coord.update();
    
    EXPECT_DOUBLE_EQ(50.0, coord.getBrewWeight());
    
    // Stop tracking
    coord.stopBrewWeightTracking();
    EXPECT_FALSE(coord.isBrewWeightTrackingActive());
    EXPECT_DOUBLE_EQ(0.0, coord.getBrewWeight());
}

TEST(SensorCoordinatorTest, WaterTankSensor) {
    // Note: Water tank sensor is a Switch, not ISensor
    // This test verifies the coordinator handles it correctly
    SensorCoordinator coord(nullptr, nullptr, nullptr);
    
    // Default should assume full (safe default)
    EXPECT_TRUE(coord.isWaterTankFull());
    
    // Update should not crash without sensor
    coord.update();
}

TEST(SensorCoordinatorTest, ScaleTareMode) {
    SensorCoordinator coord;
    
    EXPECT_FALSE(coord.isScaleTareMode());
    
    coord.setScaleTareMode(true);
    EXPECT_TRUE(coord.isScaleTareMode());
    
    coord.setScaleTareMode(false);
    EXPECT_FALSE(coord.isScaleTareMode());
}

TEST(SensorCoordinatorTest, ScaleCalibrationMode) {
    SensorCoordinator coord;
    
    EXPECT_FALSE(coord.isScaleCalibrationMode());
    
    coord.setScaleCalibrationMode(true);
    EXPECT_TRUE(coord.isScaleCalibrationMode());
    
    coord.setScaleCalibrationMode(false);
    EXPECT_FALSE(coord.isScaleCalibrationMode());
}

TEST(SensorCoordinatorTest, MultipleSensorErrors) {
    auto mockTemp = std::make_unique<MockSensor>();
    auto mockScale = std::make_unique<MockSensor>();
    mockTemp->setShouldFail(true);
    mockScale->setShouldFail(true);
    
    SensorCoordinator coord(mockTemp.get(), mockScale.get(), nullptr);
    
    coord.update();
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    coord.update();
    
    EXPECT_TRUE(coord.hasTemperatureSensorError());
    EXPECT_TRUE(coord.hasScaleSensorError());
    EXPECT_TRUE(coord.hasSensorError());
}

TEST(SensorCoordinatorTest, SensorRecovery) {
    auto mockTemp = std::make_unique<MockSensor>(95.5);
    mockTemp->setShouldFail(true);  // Start with error
    
    SensorCoordinator coord(mockTemp.get(), nullptr, nullptr);
    
    coord.update();
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    coord.update();
    
    EXPECT_TRUE(coord.hasTemperatureSensorError());
    
    // Recover from error
    mockTemp->setShouldFail(false);
    coord.update();
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    coord.update();
    
    EXPECT_FALSE(coord.hasTemperatureSensorError());
    EXPECT_DOUBLE_EQ(95.5, coord.getTemperature());
}
