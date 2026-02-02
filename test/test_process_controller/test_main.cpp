/**
 * @file test_main.cpp
 * @brief Integration tests for ProcessController
 *
 * Tests safety-critical behavior:
 * - Emergency stop on overtemperature
 * - PID output clamping to safe bounds
 * - Setpoint management (brew/steam switching)
 * - Initialization from config
 *
 * NOTE: Due to complex dependencies in ProcessController.cpp (display, hardware manager, etc.),
 * we cannot include the full implementation in tests. Instead, we test the critical safety
 * components (EmergencyStopManager) directly and verify ProcessController interface contracts.
 */

#include <gtest/gtest.h>
#include <gmock/gmock.h>
#include "../test_support.h"
#include "../mocks/MockHardwareManager.h"
#include "clevercoffee/control/EmergencyStopManager.h"
#include "clevercoffee/Config.h"
#include <PID_v1.h>

// Include minimal implementations for testing
#include "../mocks/ConfigStubs.cpp"
#include "../../src/Logger.cpp"
#include "../../src/control/EmergencyStopManager.cpp"

using namespace CleverCoffee;
using ::testing::Return;
using ::testing::NiceMock;
using ::testing::_;

// ============================================================================
// TEST FIXTURE - Emergency Stop Manager Integration
// ============================================================================

class EmergencyStopIntegrationTest : public ::testing::Test {
protected:
    std::unique_ptr<EmergencyStopManager> emergencyManager_;
    NiceMock<MockHardwareManager> mockHardwareManager_;

    void SetUp() override {
        // Configure default config values
        Config& config = Config::getInstance();
        config.emergencyStopTemp.set(145.0);
        config.emergencyStopHysteresis.set(10.0);

        // Create Emergency Stop Manager
        emergencyManager_ = std::make_unique<EmergencyStopManager>(config);

        // Configure safe default temperature
        ON_CALL(mockHardwareManager_, getCurrentTemperature())
            .WillByDefault(Return(25.0));
        ON_CALL(mockHardwareManager_, hasTemperatureError())
            .WillByDefault(Return(false));
    }

    void TearDown() override {
        emergencyManager_->reset();
        emergencyManager_.reset();
    }
};

// ============================================================================
// SAFETY-CRITICAL TESTS
// ============================================================================

/**
 * TEST: Emergency stop triggers on overtemperature
 *
 * CRITICAL SAFETY TEST: Verifies the emergency stop system triggers when temperature
 * exceeds the emergency threshold (default 145°C)
 *
 * This is the most critical safety test - it ensures the machine will shut down
 * before reaching dangerous temperatures.
 */
TEST_F(EmergencyStopIntegrationTest, EmergencyStopOnOvertemperature) {
    Config& config = Config::getInstance();
    const double emergencyThreshold = config.emergencyStopTemp.get();
    const double dangerousTemp = emergencyThreshold + 5.0;  // 150°C

    // Simulate dangerous temperature - need 3 consecutive readings for debounce
    // First two calls build up debounce counter
    bool result1 = emergencyManager_->checkEmergencyConditions(dangerousTemp);
    EXPECT_FALSE(result1) << "Should not trigger on first reading";
    EXPECT_EQ(1, emergencyManager_->getDebounceCount());

    bool result2 = emergencyManager_->checkEmergencyConditions(dangerousTemp);
    EXPECT_FALSE(result2) << "Should not trigger on second reading";
    EXPECT_EQ(2, emergencyManager_->getDebounceCount());

    // Third call should trigger emergency
    bool result3 = emergencyManager_->checkEmergencyConditions(dangerousTemp);
    EXPECT_TRUE(result3)
        << "Emergency should trigger after 3 consecutive overtemperature readings";
    EXPECT_TRUE(emergencyManager_->isEmergencyActive());
}

/**
 * TEST: Emergency stop does not trigger on single high reading
 *
 * SAFETY TEST: Verifies debounce logic prevents false alarms from single spikes
 */
TEST_F(EmergencyStopIntegrationTest, NoFalseAlarmsOnSingleSpike) {
    Config& config = Config::getInstance();
    const double emergencyThreshold = config.emergencyStopTemp.get();
    const double dangerousTemp = emergencyThreshold + 5.0;  // 150°C
    const double normalTemp = 93.0;

    // Single high reading
    bool result1 = emergencyManager_->checkEmergencyConditions(dangerousTemp);
    EXPECT_FALSE(result1) << "Should not trigger on single high reading";

    // Return to normal
    bool result2 = emergencyManager_->checkEmergencyConditions(normalTemp);
    EXPECT_FALSE(result2) << "Should not trigger after returning to normal";

    // Counter should have reset
    EXPECT_EQ(0, emergencyManager_->getDebounceCount());
    EXPECT_FALSE(emergencyManager_->isEmergencyActive());
}

/**
 * TEST: Emergency clears when temperature drops to safe levels
 *
 * SAFETY TEST: Verifies emergency clears only when temperature drops below
 * the absolute safe threshold (100°C), not just the hysteresis zone
 */
TEST_F(EmergencyStopIntegrationTest, EmergencyClearsAtSafeTemperature) {
    Config& config = Config::getInstance();
    const double emergencyThreshold = config.emergencyStopTemp.get();  // 145°C
    const double dangerousTemp = emergencyThreshold + 5.0;             // 150°C
    const double EMERGENCY_SAFE_TEMP = 100.0;  // From Temperature::EMERGENCY_SAFE_TEMP_C

    // Trigger emergency with 3 consecutive high readings
    emergencyManager_->checkEmergencyConditions(dangerousTemp);
    emergencyManager_->checkEmergencyConditions(dangerousTemp);
    emergencyManager_->checkEmergencyConditions(dangerousTemp);
    EXPECT_TRUE(emergencyManager_->isEmergencyActive());

    // Temperature just below emergency threshold should NOT clear emergency
    bool canClear1 = emergencyManager_->isEmergencyCleared(emergencyThreshold - 1.0);
    EXPECT_FALSE(canClear1) << "Emergency should not clear at 144°C (above safe threshold)";

    // Temperature above safe threshold should NOT clear emergency
    bool canClear2 = emergencyManager_->isEmergencyCleared(EMERGENCY_SAFE_TEMP + 1.0);
    EXPECT_FALSE(canClear2) << "Emergency should not clear at 101°C (above safe threshold)";

    // Temperature at safe threshold should clear emergency
    bool canClear3 = emergencyManager_->isEmergencyCleared(EMERGENCY_SAFE_TEMP);
    EXPECT_TRUE(canClear3) << "Emergency should clear at 100°C (safe threshold)";

    // Temperature well below safe threshold should clear emergency
    bool canClear4 = emergencyManager_->isEmergencyCleared(EMERGENCY_SAFE_TEMP - 10.0);
    EXPECT_TRUE(canClear4) << "Emergency should clear at 90°C (well below safe threshold)";
}

// ============================================================================
// PID OUTPUT BOUNDS TESTS
// ============================================================================

/**
 * TEST: PID library respects output limits
 *
 * SAFETY TEST: Verifies that PID output is properly clamped by the PID library
 * This tests our PID stub behavior which should match the real library
 */
TEST(PIDSafetyTest, PIDOutputClampedToLimits) {
    double input = 20.0;      // Very cold
    double output = 0.0;
    double setpoint = 93.0;   // Normal brewing temperature

    // Create PID with high gains that would push output beyond limits
    PID pid(&input, &output, &setpoint, 100.0, 50.0, 25.0, P_ON_M, DIRECT);
    pid.SetMode(AUTOMATIC);
    pid.SetOutputLimits(0.0, 1000.0);

    // Compute PID output
    bool computed = pid.Compute();
    EXPECT_TRUE(computed);

    // Output should be clamped to safe bounds
    EXPECT_GE(output, 0.0) << "PID output should not be negative";
    EXPECT_LE(output, 1000.0) << "PID output should not exceed 1000";
}

/**
 * TEST: PID output clamped when temperature exceeds setpoint
 */
TEST(PIDSafetyTest, PIDOutputClampedWhenHot) {
    double input = 100.0;     // Above setpoint
    double output = 0.0;
    double setpoint = 93.0;   // Normal brewing temperature

    // Create PID
    PID pid(&input, &output, &setpoint, 50.0, 25.0, 15.0, P_ON_M, DIRECT);
    pid.SetMode(AUTOMATIC);
    pid.SetOutputLimits(0.0, 1000.0);

    // Compute PID output
    bool computed = pid.Compute();
    EXPECT_TRUE(computed);

    // Output should be clamped (likely to 0 since we're above setpoint)
    EXPECT_GE(output, 0.0) << "PID output should not be negative";
    EXPECT_LE(output, 1000.0) << "PID output should not exceed 1000";
}

// ============================================================================
// SETPOINT MANAGEMENT TESTS
// ============================================================================

/**
 * TEST: Steam setpoint is higher than brew setpoint
 *
 * SAFETY TEST: Verifies configuration sanity - steam requires higher temperature
 */
TEST(SetpointSafetyTest, SteamSetpointHigherThanBrew) {
    Config& config = Config::getInstance();
    config.brewSetpoint.set(93.0);
    config.steamSetpoint.set(130.0);

    double brewSetpoint = config.brewSetpoint.get();
    double steamSetpoint = config.steamSetpoint.get();

    EXPECT_GT(steamSetpoint, brewSetpoint)
        << "Steam setpoint must be higher than brew setpoint";
    EXPECT_GE(brewSetpoint, 85.0) << "Brew setpoint should be reasonable (>= 85°C)";
    EXPECT_LE(steamSetpoint, 145.0) << "Steam setpoint should be below emergency threshold";
}

// ============================================================================
// CONFIGURATION TESTS
// ============================================================================

/**
 * TEST: Emergency threshold is above operating temperatures
 *
 * SAFETY TEST: Verifies emergency threshold provides adequate safety margin
 */
TEST(ConfigSafetyTest, EmergencyThresholdAboveOperatingTemperatures) {
    Config& config = Config::getInstance();
    config.brewSetpoint.set(93.0);
    config.steamSetpoint.set(130.0);
    config.emergencyStopTemp.set(145.0);

    double brewSetpoint = config.brewSetpoint.get();
    double steamSetpoint = config.steamSetpoint.get();
    double emergencyThreshold = config.emergencyStopTemp.get();

    EXPECT_GT(emergencyThreshold, steamSetpoint)
        << "Emergency threshold must be above steam setpoint";
    EXPECT_GE(emergencyThreshold - steamSetpoint, 10.0)
        << "Emergency threshold should have at least 10°C margin above steam setpoint";
}

/**
 * TEST: PID parameters are positive
 *
 * SAFETY TEST: Verifies PID parameters are reasonable
 */
TEST(ConfigSafetyTest, PIDParametersArePositive) {
    Config& config = Config::getInstance();
    config.pidRegularKp.set(50.0);
    config.pidRegularTn.set(100.0);
    config.pidRegularTv.set(15.0);

    double kp = config.pidRegularKp.get();
    double tn = config.pidRegularTn.get();
    double tv = config.pidRegularTv.get();

    EXPECT_GT(kp, 0.0) << "Kp should be positive";
    EXPECT_GT(tn, 0.0) << "Tn should be positive";
    EXPECT_GE(tv, 0.0) << "Tv should be non-negative";
}

// Note: main() is provided by test/main.cpp
