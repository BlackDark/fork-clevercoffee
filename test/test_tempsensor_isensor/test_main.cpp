/**
 * @file test_main.cpp
 * @brief Unit tests for TempSensor ISensor implementation
 *
 * Tests verify that TempSensor correctly implements the ISensor interface
 * with async read patterns, timeout handling, and error reporting.
 */

#include <gtest/gtest.h>
#include "clevercoffee/hardware/tempsensors/TempSensor.h"
#include "clevercoffee/sensors/ISensor.h"
#include "clevercoffee/errors/ErrorCodes.h"
#include <thread>
#include <chrono>

using namespace CleverCoffee;

// Mock TempSensor for testing
class MockTempSensor : public TempSensor {
protected:
    bool sample_temperature(double& temperature) const override {
        if (shouldFail_) {
            return false;
        }
        temperature = mockTemperature_;
        return true;
    }

public:
    void setMockTemperature(double temp) { mockTemperature_ = temp; }
    void setShouldFail(bool fail) { shouldFail_ = fail; }
    
    const char* getSensorType() const noexcept override {
        return "MockTempSensor";
    }

private:
    mutable double mockTemperature_ = 95.0;
    mutable bool shouldFail_ = false;
};

// ============================================================================
// ISensor Interface Implementation Tests
// ============================================================================

class TempSensorISensorTest : public ::testing::Test {
protected:
    MockTempSensor sensor;
    
    void SetUp() override {
        sensor.setMockTemperature(95.0);
        sensor.setShouldFail(false);
    }
};

/**
 * Test that ISensor interface methods exist and can be called
 */
TEST_F(TempSensorISensorTest, ImplementsISensorInterface) {
    ISensor* iface = &sensor;
    ASSERT_NE(iface, nullptr);
    
    // Should be able to call interface methods
    iface->startRead();
    auto result = iface->tryGetValue();
    EXPECT_NE(iface->getSensorType(), nullptr);
    EXPECT_TRUE(iface->isConnected());
}

/**
 * Test that startRead() initiates async read
 */
TEST_F(TempSensorISensorTest, StartReadInitiatesAsyncRead) {
    sensor.startRead();
    
    // After startRead, tryGetValue should eventually return a value
    auto result = sensor.tryGetValue();
    
    // Should get either success or NOT_READY (not an error)
    if (!result) {
        EXPECT_EQ(result.error().code(), ErrorCode::SENSOR_NOT_READY);
    }
}

/**
 * Test successful temperature read
 */
TEST_F(TempSensorISensorTest, SuccessfulRead) {
    sensor.setMockTemperature(93.5);
    sensor.startRead();
    
    auto result = sensor.tryGetValue();
    
    ASSERT_TRUE(result.hasValue());
    EXPECT_DOUBLE_EQ(result.value(), 93.5);
}

/**
 * Test that tryGetValue returns NOT_READY when read not started
 */
TEST_F(TempSensorISensorTest, NotReadyWithoutStartRead) {
    // Don't call startRead()
    auto result = sensor.tryGetValue();
    
    ASSERT_FALSE(result.hasValue());
    EXPECT_EQ(result.error().code(), ErrorCode::SENSOR_NOT_READY);
}

/**
 * Test timeout handling for slow sensors
 */
TEST_F(TempSensorISensorTest, TimeoutOnSlowRead) {
    sensor.startRead();
    
    // Wait longer than timeout (1000ms)
    std::this_thread::sleep_for(std::chrono::milliseconds(1100));
    
    auto result = sensor.tryGetValue();
    
    ASSERT_FALSE(result.hasValue());
    EXPECT_EQ(result.error().code(), ErrorCode::SENSOR_TIMEOUT);
}

/**
 * Test multiple consecutive reads
 */
TEST_F(TempSensorISensorTest, MultipleConsecutiveReads) {
    sensor.setMockTemperature(92.0);
    sensor.startRead();
    auto result1 = sensor.tryGetValue();
    ASSERT_TRUE(result1.hasValue());
    EXPECT_DOUBLE_EQ(result1.value(), 92.0);
    
    // Second read
    sensor.setMockTemperature(93.0);
    sensor.startRead();
    auto result2 = sensor.tryGetValue();
    ASSERT_TRUE(result2.hasValue());
    EXPECT_DOUBLE_EQ(result2.value(), 93.0);
    
    // Third read
    sensor.setMockTemperature(94.0);
    sensor.startRead();
    auto result3 = sensor.tryGetValue();
    ASSERT_TRUE(result3.hasValue());
    EXPECT_DOUBLE_EQ(result3.value(), 94.0);
}

/**
 * Test that failed sensor reads are reported correctly
 */
TEST_F(TempSensorISensorTest, FailedReadReturnsNotReady) {
    sensor.setShouldFail(true);
    sensor.startRead();
    
    auto result = sensor.tryGetValue();
    
    ASSERT_FALSE(result.hasValue());
    EXPECT_EQ(result.error().code(), ErrorCode::SENSOR_NOT_READY);
}

/**
 * Test getSensorType returns valid string
 */
TEST_F(TempSensorISensorTest, GetSensorTypeReturnsValidString) {
    const char* type = sensor.getSensorType();
    
    ASSERT_NE(type, nullptr);
    EXPECT_STREQ(type, "MockTempSensor");
}

/**
 * Test isConnected reflects sensor error state
 */
TEST_F(TempSensorISensorTest, IsConnectedReflectsErrorState) {
    // Initially connected
    EXPECT_TRUE(sensor.isConnected());
    
    // Fail multiple reads to trigger error state
    sensor.setShouldFail(true);
    for (int i = 0; i < 15; i++) {
        sensor.updateTemperature();
    }
    
    // Should now report disconnected
    EXPECT_FALSE(sensor.isConnected());
}

// ============================================================================
// Legacy Interface Compatibility Tests
// ============================================================================

class TempSensorLegacyTest : public ::testing::Test {
protected:
    MockTempSensor sensor;
};

/**
 * Test that legacy updateTemperature() still works
 */
TEST_F(TempSensorLegacyTest, LegacyUpdateTemperatureWorks) {
    sensor.setMockTemperature(96.5);
    
    bool success = sensor.updateTemperature();
    
    EXPECT_TRUE(success);
    EXPECT_DOUBLE_EQ(sensor.getCurrentTemperature(), 96.5);
}

/**
 * Test that legacy getCurrentTemperature() returns cached value
 */
TEST_F(TempSensorLegacyTest, GetCurrentTemperatureReturnsCached) {
    sensor.setMockTemperature(97.0);
    sensor.updateTemperature();
    
    double temp = sensor.getCurrentTemperature();
    
    EXPECT_DOUBLE_EQ(temp, 97.0);
}

/**
 * Test that legacy hasError() detects sensor failures
 */
TEST_F(TempSensorLegacyTest, HasErrorDetectsFailures) {
    // Initially no error
    EXPECT_FALSE(sensor.hasError());
    
    // Trigger error by failing multiple reads
    sensor.setShouldFail(true);
    for (int i = 0; i < 15; i++) {
        sensor.updateTemperature();
    }
    
    EXPECT_TRUE(sensor.hasError());
}

// ============================================================================
// Error Handling Tests
// ============================================================================

class TempSensorErrorHandlingTest : public ::testing::Test {
protected:
    MockTempSensor sensor;
};

/**
 * Test error recovery after sensor reconnect
 */
TEST_F(TempSensorErrorHandlingTest, ErrorRecoveryAfterReconnect) {
    // Trigger error
    sensor.setShouldFail(true);
    for (int i = 0; i < 15; i++) {
        sensor.updateTemperature();
    }
    EXPECT_TRUE(sensor.hasError());
    
    // Recover
    sensor.setShouldFail(false);
    sensor.setMockTemperature(95.0);
    sensor.updateTemperature();
    
    EXPECT_FALSE(sensor.hasError());
    EXPECT_DOUBLE_EQ(sensor.getCurrentTemperature(), 95.0);
}

/**
 * Test that isValidTemperature filters invalid readings
 */
TEST_F(TempSensorErrorHandlingTest, ValidTemperatureRangeChecking) {
    EXPECT_TRUE(TempSensor::isValidTemperature(0.0));
    EXPECT_TRUE(TempSensor::isValidTemperature(25.0));
    EXPECT_TRUE(TempSensor::isValidTemperature(95.0));
    EXPECT_TRUE(TempSensor::isValidTemperature(120.0));
    
    EXPECT_FALSE(TempSensor::isValidTemperature(-60.0));
    EXPECT_FALSE(TempSensor::isValidTemperature(160.0));
}

// ============================================================================
// Thread Safety Tests
// ============================================================================

class TempSensorThreadSafetyTest : public ::testing::Test {
protected:
    MockTempSensor sensor;
};

/**
 * Test concurrent reads from multiple threads
 */
TEST_F(TempSensorThreadSafetyTest, ConcurrentReads) {
    sensor.setMockTemperature(95.0);
    
    std::thread t1([this]() {
        for (int i = 0; i < 100; i++) {
            sensor.startRead();
            sensor.tryGetValue();
        }
    });
    
    std::thread t2([this]() {
        for (int i = 0; i < 100; i++) {
            sensor.startRead();
            sensor.tryGetValue();
        }
    });
    
    t1.join();
    t2.join();
    
    // Should not crash or deadlock
    SUCCEED();
}

/**
 * Test mixed ISensor and legacy interface calls
 */
TEST_F(TempSensorThreadSafetyTest, MixedInterfaceCalls) {
    sensor.setMockTemperature(95.0);
    
    std::thread t1([this]() {
        for (int i = 0; i < 50; i++) {
            sensor.startRead();
            sensor.tryGetValue();
        }
    });
    
    std::thread t2([this]() {
        for (int i = 0; i < 50; i++) {
            sensor.updateTemperature();
            sensor.getCurrentTemperature();
        }
    });
    
    t1.join();
    t2.join();
    
    // Both interfaces should work without interference
    SUCCEED();
}
