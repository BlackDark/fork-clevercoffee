/**
 * @file test/test_process_controller/test_process_controller.cpp
 * @brief ProcessController tests - testing real production code
 *
 * These tests verify state classification functions that ProcessController
 * depends on. We test REAL production functions, not duplicated logic.
 *
 * NOTE on ProcessController::shouldPIDBeEnabled():
 * This private method depends on global state (g_state.process.brewPidDisabled),
 * making it difficult to unit test directly without complex mocking and global
 * state initialization. Instead, we verify the underlying state classification
 * functions that it depends on:
 * - isBrewState()
 * - isSteamState()
 * - isHotWaterState()
 * - isBackflushState()
 *
 * From ProcessController.cpp:253-259:
 * bool ProcessController::shouldPIDBeEnabled(MachineStateId machineState) const {
 *     return !(machineState == MachineStateId::PID_DISABLED || 
 *              machineState == MachineStateId::WATER_TANK_EMPTY ||
 *              machineState == MachineStateId::SENSOR_ERROR || 
 *              machineState == MachineStateId::EMERGENCY_STOP ||
 *              machineState == MachineStateId::EEPROM_ERROR || 
 *              machineState == MachineStateId::STANDBY ||
 *              isBackflushState(machineState) || 
 *              g_state.process.brewPidDisabled);  // Runtime flag
 * }
 */

#include <gtest/gtest.h>

// IMPORTANT: Import REAL production code
#include "../../include/clevercoffee/state/MachineStateIds.h"


/**
 * Test Suite: State classification functions used by ProcessController
 *
 * These tests verify the real state classification functions that
 * ProcessController::shouldPIDBeEnabled() depends on. All tests call
 * REAL production functions, never test duplicated logic.
 */
class ProcessControllerStateClassification_Test : public ::testing::Test {
  protected:
    // States that should be classified as brew states
    static constexpr MachineStateId BREW_STATES[] = {
        MachineStateId::BREW_IDLE,
        MachineStateId::BREW_PREINFUSION,
        MachineStateId::BREW_RUNNING,
    };

    // States that should be classified as steam states
    static constexpr MachineStateId STEAM_STATES[] = {
        MachineStateId::STEAM_IDLE,
        MachineStateId::STEAM_RUNNING,
    };

    // States that should be classified as hot water states
    static constexpr MachineStateId HOT_WATER_STATES[] = {
        MachineStateId::HOT_WATER_IDLE,
        MachineStateId::HOT_WATER_RUNNING,
    };

    // States that should be classified as backflush states
    static constexpr MachineStateId BACKFLUSH_STATES[] = {
        MachineStateId::BACKFLUSH_IDLE,
        MachineStateId::BACKFLUSH_FILLING,
        MachineStateId::BACKFLUSH_FLUSHING,
        MachineStateId::BACKFLUSH_FINISHED,
    };
};

// Test 1: Verify brew state classification (uses REAL isBrewState function)
TEST_F(ProcessControllerStateClassification_Test, BrewStatesIdentified) {
    for (auto state : BREW_STATES) {
        EXPECT_TRUE(isBrewState(state))
            << "State " << static_cast<int>(state) << " should be classified as brew";
    }
}

// Test 2: Verify steam state classification (uses REAL isSteamState function)
TEST_F(ProcessControllerStateClassification_Test, SteamStatesIdentified) {
    for (auto state : STEAM_STATES) {
        EXPECT_TRUE(isSteamState(state))
            << "State " << static_cast<int>(state) << " should be classified as steam";
    }
}

// Test 3: Verify hot water state classification (uses REAL isHotWaterState function)
TEST_F(ProcessControllerStateClassification_Test, HotWaterStatesIdentified) {
    for (auto state : HOT_WATER_STATES) {
        EXPECT_TRUE(isHotWaterState(state))
            << "State " << static_cast<int>(state) << " should be classified as hot water";
    }
}

// Test 4: Verify backflush state classification (uses REAL isBackflushState function)
TEST_F(ProcessControllerStateClassification_Test, BackflushStatesIdentified) {
    for (auto state : BACKFLUSH_STATES) {
        EXPECT_TRUE(isBackflushState(state))
            << "State " << static_cast<int>(state) << " should be classified as backflush";
    }
}

// Test 5: Verify manual flush states are NOT backflush
TEST_F(ProcessControllerStateClassification_Test, ManualFlushNotBackflush) {
    EXPECT_FALSE(isBackflushState(MachineStateId::MANUAL_FLUSH_IDLE));
    EXPECT_FALSE(isBackflushState(MachineStateId::MANUAL_FLUSH_RUNNING));
}

// Test 6: Verify PID_NORMAL is not any specific type
TEST_F(ProcessControllerStateClassification_Test, PIDNormalClassification) {
    EXPECT_FALSE(isBrewState(MachineStateId::PID_NORMAL));
    EXPECT_FALSE(isSteamState(MachineStateId::PID_NORMAL));
    EXPECT_FALSE(isHotWaterState(MachineStateId::PID_NORMAL));
    EXPECT_FALSE(isBackflushState(MachineStateId::PID_NORMAL));
}

// Test 7: Verify INIT state is not any specific type
TEST_F(ProcessControllerStateClassification_Test, InitClassification) {
    EXPECT_FALSE(isBrewState(MachineStateId::INIT));
    EXPECT_FALSE(isSteamState(MachineStateId::INIT));
    EXPECT_FALSE(isHotWaterState(MachineStateId::INIT));
    EXPECT_FALSE(isBackflushState(MachineStateId::INIT));
}

// Test 8: Verify error states are not operational states
TEST_F(ProcessControllerStateClassification_Test, ErrorStatesNotOperational) {
    EXPECT_FALSE(isBrewState(MachineStateId::SENSOR_ERROR));
    EXPECT_FALSE(isBrewState(MachineStateId::EEPROM_ERROR));
    EXPECT_FALSE(isBrewState(MachineStateId::EMERGENCY_STOP));
}
