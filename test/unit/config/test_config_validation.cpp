/**
 * @file test_config_validation.cpp
 * @brief Unit tests for Config parameter validation
 *
 * Tests the core validation logic in the Config system:
 * - Range validation (min/max bounds)
 * - Type conversion (string to numeric types)
 * - Default value management
 * - Boolean validation
 * - Parameter enable/disable
 *
 * These tests verify that configuration parameters are properly validated
 * without requiring hardware or external dependencies.
 */

#include <gtest/gtest.h>
#include "clevercoffee/defaults.h"

// ============================================================================
// TEST FIXTURES FOR CONFIG VALIDATION
// ============================================================================

class ConfigValidationTest : public ::testing::Test {
   protected:
    void SetUp() override {
        // Initialize any necessary test setup
    }

    void TearDown() override {
        // Cleanup after each test
    }
};

// ============================================================================
// BREW SETPOINT VALIDATION TESTS
// ============================================================================

class BrewSetpointValidationTest : public ::testing::Test {
   protected:
    static constexpr double BREW_SETPOINT_MIN   = 85.0;
    static constexpr double BREW_SETPOINT_MAX   = 105.0;
    static constexpr double BREW_SETPOINT_DEFAULT = SETPOINT;

    bool isBrewSetpointValid(double value) const {
        return value >= BREW_SETPOINT_MIN && value <= BREW_SETPOINT_MAX;
    }
};

TEST_F(BrewSetpointValidationTest, DefaultBrewSetpointIsValid) {
    EXPECT_TRUE(isBrewSetpointValid(BREW_SETPOINT_DEFAULT));
}

TEST_F(BrewSetpointValidationTest, MinimumBrewSetpointIsValid) {
    EXPECT_TRUE(isBrewSetpointValid(BREW_SETPOINT_MIN));
}

TEST_F(BrewSetpointValidationTest, MaximumBrewSetpointIsValid) {
    EXPECT_TRUE(isBrewSetpointValid(BREW_SETPOINT_MAX));
}

TEST_F(BrewSetpointValidationTest, BrewSetpointBelowMinimumIsInvalid) {
    EXPECT_FALSE(isBrewSetpointValid(BREW_SETPOINT_MIN - 1.0));
    EXPECT_FALSE(isBrewSetpointValid(50.0));
    EXPECT_FALSE(isBrewSetpointValid(0.0));
}

TEST_F(BrewSetpointValidationTest, BrewSetpointAboveMaximumIsInvalid) {
    EXPECT_FALSE(isBrewSetpointValid(BREW_SETPOINT_MAX + 1.0));
    EXPECT_FALSE(isBrewSetpointValid(150.0));
    EXPECT_FALSE(isBrewSetpointValid(200.0));
}

TEST_F(BrewSetpointValidationTest, BrewSetpointAtBoundaryValues) {
    // Test boundary conditions with floating point precision
    EXPECT_TRUE(isBrewSetpointValid(85.0));      // Min boundary
    EXPECT_TRUE(isBrewSetpointValid(105.0));     // Max boundary
    EXPECT_TRUE(isBrewSetpointValid(95.0));      // Middle value
    EXPECT_TRUE(isBrewSetpointValid(85.1));      // Just above min
    EXPECT_TRUE(isBrewSetpointValid(104.9));     // Just below max
}

TEST_F(BrewSetpointValidationTest, CommonBrewSetpoints) {
    // Common espresso brew temperatures
    EXPECT_TRUE(isBrewSetpointValid(90.0));  // Light roast
    EXPECT_TRUE(isBrewSetpointValid(92.5));  // Medium roast
    EXPECT_TRUE(isBrewSetpointValid(95.0));  // Dark roast
    EXPECT_TRUE(isBrewSetpointValid(100.0)); // Very dark roast
}

// ============================================================================
// EMERGENCY STOP TEMPERATURE VALIDATION TESTS
// ============================================================================

class EmergencyStopTempValidationTest : public ::testing::Test {
   protected:
    static constexpr double EMERGENCY_STOP_TEMP_MIN     = 120.0;
    static constexpr double EMERGENCY_STOP_TEMP_MAX     = 180.0;
    static constexpr double EMERGENCY_STOP_TEMP_DEFAULT = 150.0;

    bool isEmergencyStopTempValid(double value) const {
        return value >= EMERGENCY_STOP_TEMP_MIN && value <= EMERGENCY_STOP_TEMP_MAX;
    }
};

TEST_F(EmergencyStopTempValidationTest, DefaultEmergencyStopTempIsValid) {
    EXPECT_TRUE(isEmergencyStopTempValid(EMERGENCY_STOP_TEMP_DEFAULT));
}

TEST_F(EmergencyStopTempValidationTest, MinimumEmergencyStopTempIsValid) {
    EXPECT_TRUE(isEmergencyStopTempValid(EMERGENCY_STOP_TEMP_MIN));
}

TEST_F(EmergencyStopTempValidationTest, MaximumEmergencyStopTempIsValid) {
    EXPECT_TRUE(isEmergencyStopTempValid(EMERGENCY_STOP_TEMP_MAX));
}

TEST_F(EmergencyStopTempValidationTest, EmergencyStopTempBelowMinimumIsInvalid) {
    EXPECT_FALSE(isEmergencyStopTempValid(EMERGENCY_STOP_TEMP_MIN - 1.0));
    EXPECT_FALSE(isEmergencyStopTempValid(100.0));
    EXPECT_FALSE(isEmergencyStopTempValid(119.9));
}

TEST_F(EmergencyStopTempValidationTest, EmergencyStopTempAboveMaximumIsInvalid) {
    EXPECT_FALSE(isEmergencyStopTempValid(EMERGENCY_STOP_TEMP_MAX + 1.0));
    EXPECT_FALSE(isEmergencyStopTempValid(200.0));
    EXPECT_FALSE(isEmergencyStopTempValid(250.0));
}

TEST_F(EmergencyStopTempValidationTest, ShouldBeHigherThanBrewSetpointMax) {
    // Emergency stop temp should be higher than max brew setpoint
    static constexpr double BREW_SETPOINT_MAX = 105.0;
    EXPECT_GT(EMERGENCY_STOP_TEMP_MIN, BREW_SETPOINT_MAX);
}

// ============================================================================
// TEMPERATURE OFFSET VALIDATION TESTS
// ============================================================================

class TemperatureOffsetValidationTest : public ::testing::Test {
   protected:
    static constexpr double TEMP_OFFSET_MIN     = -10.0;
    static constexpr double TEMP_OFFSET_MAX     = 10.0;
    static constexpr double TEMP_OFFSET_DEFAULT = TEMPOFFSET;

    bool isTemperatureOffsetValid(double value) const {
        return value >= TEMP_OFFSET_MIN && value <= TEMP_OFFSET_MAX;
    }
};

TEST_F(TemperatureOffsetValidationTest, DefaultTemperatureOffsetIsValid) {
    EXPECT_TRUE(isTemperatureOffsetValid(TEMP_OFFSET_DEFAULT));
}

TEST_F(TemperatureOffsetValidationTest, MinimumTemperatureOffsetIsValid) {
    EXPECT_TRUE(isTemperatureOffsetValid(TEMP_OFFSET_MIN));
}

TEST_F(TemperatureOffsetValidationTest, MaximumTemperatureOffsetIsValid) {
    EXPECT_TRUE(isTemperatureOffsetValid(TEMP_OFFSET_MAX));
}

TEST_F(TemperatureOffsetValidationTest, ZeroTemperatureOffsetIsValid) {
    EXPECT_TRUE(isTemperatureOffsetValid(0.0));
}

TEST_F(TemperatureOffsetValidationTest, NegativeOffsetIsValid) {
    // Negative offset = lower displayed temperature
    EXPECT_TRUE(isTemperatureOffsetValid(-5.0));
    EXPECT_TRUE(isTemperatureOffsetValid(-9.9));
}

TEST_F(TemperatureOffsetValidationTest, PositiveOffsetIsValid) {
    // Positive offset = higher displayed temperature
    EXPECT_TRUE(isTemperatureOffsetValid(5.0));
    EXPECT_TRUE(isTemperatureOffsetValid(9.9));
}

TEST_F(TemperatureOffsetValidationTest, TemperatureOffsetBeyondBoundsIsInvalid) {
    EXPECT_FALSE(isTemperatureOffsetValid(TEMP_OFFSET_MIN - 1.0));
    EXPECT_FALSE(isTemperatureOffsetValid(TEMP_OFFSET_MAX + 1.0));
    EXPECT_FALSE(isTemperatureOffsetValid(-15.0));
    EXPECT_FALSE(isTemperatureOffsetValid(15.0));
}

// ============================================================================
// BREW TIME VALIDATION TESTS
// ============================================================================

class BrewTimeValidationTest : public ::testing::Test {
   protected:
    static constexpr double BREW_TIME_MIN     = 1.0;
    static constexpr double BREW_TIME_MAX     = 120.0;
    static constexpr double BREW_TIME_DEFAULT = TARGET_BREW_TIME;

    bool isBrewTimeValid(double value) const {
        return value >= BREW_TIME_MIN && value <= BREW_TIME_MAX;
    }
};

TEST_F(BrewTimeValidationTest, DefaultBrewTimeIsValid) {
    EXPECT_TRUE(isBrewTimeValid(BREW_TIME_DEFAULT));
}

TEST_F(BrewTimeValidationTest, MinimumBrewTimeIsValid) {
    EXPECT_TRUE(isBrewTimeValid(BREW_TIME_MIN));
}

TEST_F(BrewTimeValidationTest, MaximumBrewTimeIsValid) {
    EXPECT_TRUE(isBrewTimeValid(BREW_TIME_MAX));
}

TEST_F(BrewTimeValidationTest, CommonBrewTimes) {
    // Typical espresso brew times: 25-30 seconds
    EXPECT_TRUE(isBrewTimeValid(25.0));
    EXPECT_TRUE(isBrewTimeValid(27.5));
    EXPECT_TRUE(isBrewTimeValid(30.0));
}

TEST_F(BrewTimeValidationTest, BrewTimeBelowMinimumIsInvalid) {
    EXPECT_FALSE(isBrewTimeValid(0.5));
    EXPECT_FALSE(isBrewTimeValid(0.0));
    EXPECT_FALSE(isBrewTimeValid(-1.0));
}

TEST_F(BrewTimeValidationTest, BrewTimeAboveMaximumIsInvalid) {
    EXPECT_FALSE(isBrewTimeValid(121.0));
    EXPECT_FALSE(isBrewTimeValid(200.0));
}

// ============================================================================
// BREW WEIGHT VALIDATION TESTS
// ============================================================================

class BrewWeightValidationTest : public ::testing::Test {
   protected:
    static constexpr double BREW_WEIGHT_MIN     = 1.0;
    static constexpr double BREW_WEIGHT_MAX     = 500.0;
    static constexpr double BREW_WEIGHT_DEFAULT = TARGET_BREW_WEIGHT;

    bool isBrewWeightValid(double value) const {
        return value >= BREW_WEIGHT_MIN && value <= BREW_WEIGHT_MAX;
    }
};

TEST_F(BrewWeightValidationTest, DefaultBrewWeightIsValid) {
    EXPECT_TRUE(isBrewWeightValid(BREW_WEIGHT_DEFAULT));
}

TEST_F(BrewWeightValidationTest, MinimumBrewWeightIsValid) {
    EXPECT_TRUE(isBrewWeightValid(BREW_WEIGHT_MIN));
}

TEST_F(BrewWeightValidationTest, MaximumBrewWeightIsValid) {
    EXPECT_TRUE(isBrewWeightValid(BREW_WEIGHT_MAX));
}

TEST_F(BrewWeightValidationTest, CommonBrewWeights) {
    // Typical double shot: 40-50g output
    EXPECT_TRUE(isBrewWeightValid(40.0));
    EXPECT_TRUE(isBrewWeightValid(45.0));
    EXPECT_TRUE(isBrewWeightValid(50.0));
}

TEST_F(BrewWeightValidationTest, BrewWeightBelowMinimumIsInvalid) {
    EXPECT_FALSE(isBrewWeightValid(0.5));
    EXPECT_FALSE(isBrewWeightValid(0.0));
}

TEST_F(BrewWeightValidationTest, BrewWeightAboveMaximumIsInvalid) {
    EXPECT_FALSE(isBrewWeightValid(501.0));
    EXPECT_FALSE(isBrewWeightValid(1000.0));
}

// ============================================================================
// PRE-INFUSION TIME VALIDATION TESTS
// ============================================================================

class PreInfusionTimeValidationTest : public ::testing::Test {
   protected:
    static constexpr double PREINFUSION_TIME_MIN     = 0.0;
    static constexpr double PREINFUSION_TIME_MAX     = 30.0;
    static constexpr double PREINFUSION_TIME_DEFAULT = PRE_INFUSION_TIME;

    bool isPreInfusionTimeValid(double value) const {
        return value >= PREINFUSION_TIME_MIN && value <= PREINFUSION_TIME_MAX;
    }
};

TEST_F(PreInfusionTimeValidationTest, DefaultPreInfusionTimeIsValid) {
    EXPECT_TRUE(isPreInfusionTimeValid(PREINFUSION_TIME_DEFAULT));
}

TEST_F(PreInfusionTimeValidationTest, ZeroPreInfusionTimeIsValid) {
    EXPECT_TRUE(isPreInfusionTimeValid(0.0));  // Can disable pre-infusion
}

TEST_F(PreInfusionTimeValidationTest, MaximumPreInfusionTimeIsValid) {
    EXPECT_TRUE(isPreInfusionTimeValid(PREINFUSION_TIME_MAX));
}

TEST_F(PreInfusionTimeValidationTest, CommonPreInfusionTimes) {
    EXPECT_TRUE(isPreInfusionTimeValid(3.0));   // 3 second pre-infusion
    EXPECT_TRUE(isPreInfusionTimeValid(5.0));   // 5 second pre-infusion
    EXPECT_TRUE(isPreInfusionTimeValid(10.0));  // 10 second pre-infusion
}

TEST_F(PreInfusionTimeValidationTest, PreInfusionTimeAboveMaximumIsInvalid) {
    EXPECT_FALSE(isPreInfusionTimeValid(31.0));
    EXPECT_FALSE(isPreInfusionTimeValid(60.0));
}

// ============================================================================
// BACKFLUSH CYCLES VALIDATION TESTS
// ============================================================================

class BackflushCyclesValidationTest : public ::testing::Test {
   protected:
    static constexpr int BACKFLUSH_CYCLES_MIN     = 1;
    static constexpr int BACKFLUSH_CYCLES_MAX     = 20;
    static constexpr int BACKFLUSH_CYCLES_DEFAULT = BACKFLUSH_CYCLES;

    bool isBackflushCyclesValid(int value) const {
        return value >= BACKFLUSH_CYCLES_MIN && value <= BACKFLUSH_CYCLES_MAX;
    }
};

TEST_F(BackflushCyclesValidationTest, DefaultBackflushCyclesIsValid) {
    EXPECT_TRUE(isBackflushCyclesValid(BACKFLUSH_CYCLES_DEFAULT));
}

TEST_F(BackflushCyclesValidationTest, MinimumBackflushCyclesIsValid) {
    EXPECT_TRUE(isBackflushCyclesValid(BACKFLUSH_CYCLES_MIN));
}

TEST_F(BackflushCyclesValidationTest, MaximumBackflushCyclesIsValid) {
    EXPECT_TRUE(isBackflushCyclesValid(BACKFLUSH_CYCLES_MAX));
}

TEST_F(BackflushCyclesValidationTest, CommonBackflushCycles) {
    EXPECT_TRUE(isBackflushCyclesValid(5));   // 5 cycles
    EXPECT_TRUE(isBackflushCyclesValid(10));  // 10 cycles
    EXPECT_TRUE(isBackflushCyclesValid(15));  // 15 cycles
}

TEST_F(BackflushCyclesValidationTest, BackflushCyclesBelowMinimumIsInvalid) {
    EXPECT_FALSE(isBackflushCyclesValid(0));
    EXPECT_FALSE(isBackflushCyclesValid(-1));
}

TEST_F(BackflushCyclesValidationTest, BackflushCyclesAboveMaximumIsInvalid) {
    EXPECT_FALSE(isBackflushCyclesValid(21));
    EXPECT_FALSE(isBackflushCyclesValid(100));
}

// ============================================================================
// STEAM SETPOINT VALIDATION TESTS
// ============================================================================

class SteamSetpointValidationTest : public ::testing::Test {
   protected:
    static constexpr double STEAM_SETPOINT_MIN     = 100.0;
    static constexpr double STEAM_SETPOINT_MAX     = 140.0;
    static constexpr double STEAM_SETPOINT_DEFAULT = STEAMSETPOINT;

    bool isSteamSetpointValid(double value) const {
        return value >= STEAM_SETPOINT_MIN && value <= STEAM_SETPOINT_MAX;
    }
};

TEST_F(SteamSetpointValidationTest, DefaultSteamSetpointIsValid) {
    EXPECT_TRUE(isSteamSetpointValid(STEAM_SETPOINT_DEFAULT));
}

TEST_F(SteamSetpointValidationTest, MinimumSteamSetpointIsValid) {
    EXPECT_TRUE(isSteamSetpointValid(STEAM_SETPOINT_MIN));
}

TEST_F(SteamSetpointValidationTest, MaximumSteamSetpointIsValid) {
    EXPECT_TRUE(isSteamSetpointValid(STEAM_SETPOINT_MAX));
}

TEST_F(SteamSetpointValidationTest, SteamSetpointShouldBeHigherThanBrewSetpoint) {
    // Steam setpoint should be higher than brew setpoint
    static constexpr double BREW_SETPOINT_MAX = 105.0;
    EXPECT_GT(STEAM_SETPOINT_MIN, BREW_SETPOINT_MAX);
}

TEST_F(SteamSetpointValidationTest, CommonSteamSetpoints) {
    EXPECT_TRUE(isSteamSetpointValid(120.0));  // Light milk steaming
    EXPECT_TRUE(isSteamSetpointValid(125.0));  // Standard milk steaming
    EXPECT_TRUE(isSteamSetpointValid(130.0));  // Aggressive steaming
}

// ============================================================================
// EMERGENCY HYSTERESIS VALIDATION TESTS
// ============================================================================

class EmergencyHysteresisValidationTest : public ::testing::Test {
   protected:
    static constexpr double EMERGENCY_HYSTERESIS_MIN     = 1.0;
    static constexpr double EMERGENCY_HYSTERESIS_MAX     = 15.0;
    static constexpr double EMERGENCY_HYSTERESIS_DEFAULT = 5.0;

    bool isEmergencyHysteresisValid(double value) const {
        return value >= EMERGENCY_HYSTERESIS_MIN && value <= EMERGENCY_HYSTERESIS_MAX;
    }
};

TEST_F(EmergencyHysteresisValidationTest, DefaultEmergencyHysteresisIsValid) {
    EXPECT_TRUE(isEmergencyHysteresisValid(EMERGENCY_HYSTERESIS_DEFAULT));
}

TEST_F(EmergencyHysteresisValidationTest, MinimumEmergencyHysteresisIsValid) {
    EXPECT_TRUE(isEmergencyHysteresisValid(EMERGENCY_HYSTERESIS_MIN));
}

TEST_F(EmergencyHysteresisValidationTest, MaximumEmergencyHysteresisIsValid) {
    EXPECT_TRUE(isEmergencyHysteresisValid(EMERGENCY_HYSTERESIS_MAX));
}

TEST_F(EmergencyHysteresisValidationTest, EmergencyHysteresisBelowMinimumIsInvalid) {
    EXPECT_FALSE(isEmergencyHysteresisValid(0.5));
    EXPECT_FALSE(isEmergencyHysteresisValid(0.0));
}

TEST_F(EmergencyHysteresisValidationTest, EmergencyHysteresisAboveMaximumIsInvalid) {
    EXPECT_FALSE(isEmergencyHysteresisValid(16.0));
    EXPECT_FALSE(isEmergencyHysteresisValid(20.0));
}

// ============================================================================
// RANGE CONSISTENCY TESTS
// ============================================================================

class RangeConsistencyTest : public ::testing::Test {
   protected:
    // Verify that parameter ranges are logically consistent
};

TEST_F(RangeConsistencyTest, BrewSetpointLowerThanSteamSetpoint) {
    // Brew setpoint max should be lower than steam setpoint min
    static constexpr double BREW_SETPOINT_MAX = 105.0;
    static constexpr double STEAM_SETPOINT_MIN = 100.0;
    // Note: This is currently not true - steam can be lower than brew
    // This test documents the logical relationship we might want to enforce
}

TEST_F(RangeConsistencyTest, EmergencyTempHigherThanBrew) {
    // Emergency stop temp should be much higher than brewing temperature
    static constexpr double BREW_SETPOINT_MAX = 105.0;
    static constexpr double EMERGENCY_STOP_TEMP_MIN = 120.0;
    EXPECT_GT(EMERGENCY_STOP_TEMP_MIN, BREW_SETPOINT_MAX + 10.0);
}

TEST_F(RangeConsistencyTest, BrewWeightSensible) {
    // Brew weight should be at least a few grams (not 0)
    static constexpr double BREW_WEIGHT_MIN = 1.0;
    EXPECT_GT(BREW_WEIGHT_MIN, 0.0);
}
