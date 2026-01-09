/**
 * @file test_machine_state_ids.cpp
 * @brief Unit tests for MachineStateIds state classification functions
 *
 * Tests the pure constexpr functions that classify machine states:
 * - isBrewState()
 * - isHotWaterState()
 * - isSteamState()
 * - isBackflushState()
 * - isManualFlushState()
 *
 * These are low-level, high-priority tests that require no hardware
 * or mocking. They test core state classification logic.
 */

#include <gtest/gtest.h>
#include "../test_support.h"
#include "clevercoffee/state/MachineStateIds.h"

// ============================================================================
// BREW STATE CLASSIFICATION TESTS
// ============================================================================

class BrewStateClassificationTest : public ::testing::Test {
   protected:
    void SetUp() override {
        // No setup needed for pure functions
    }
};

TEST_F(BrewStateClassificationTest, BrewIdleIsBrewState) {
    EXPECT_TRUE(isBrewState(MachineStateId::BREW_IDLE));
}

TEST_F(BrewStateClassificationTest, BrewPreinfusionIsBrewState) {
    EXPECT_TRUE(isBrewState(MachineStateId::BREW_PREINFUSION));
}

TEST_F(BrewStateClassificationTest, BrewPreinfusionPauseIsBrewState) {
    EXPECT_TRUE(isBrewState(MachineStateId::BREW_PREINFUSION_PAUSE));
}

TEST_F(BrewStateClassificationTest, BrewRunningIsBrewState) {
    EXPECT_TRUE(isBrewState(MachineStateId::BREW_RUNNING));
}

TEST_F(BrewStateClassificationTest, BrewFinishedIsBrewState) {
    EXPECT_TRUE(isBrewState(MachineStateId::BREW_FINISHED));
}

TEST_F(BrewStateClassificationTest, NonBrewStateIsNotBrewState) {
    EXPECT_FALSE(isBrewState(MachineStateId::INIT));
    EXPECT_FALSE(isBrewState(MachineStateId::PID_NORMAL));
    EXPECT_FALSE(isBrewState(MachineStateId::HOT_WATER_IDLE));
    EXPECT_FALSE(isBrewState(MachineStateId::STEAM_IDLE));
    EXPECT_FALSE(isBrewState(MachineStateId::BACKFLUSH_IDLE));
    EXPECT_FALSE(isBrewState(MachineStateId::WATER_TANK_EMPTY));
    EXPECT_FALSE(isBrewState(MachineStateId::EMERGENCY_STOP));
}

// ============================================================================
// HOT WATER STATE CLASSIFICATION TESTS
// ============================================================================

class HotWaterStateClassificationTest : public ::testing::Test {
   protected:
    void SetUp() override {
        // No setup needed for pure functions
    }
};

TEST_F(HotWaterStateClassificationTest, HotWaterIdleIsHotWaterState) {
    EXPECT_TRUE(isHotWaterState(MachineStateId::HOT_WATER_IDLE));
}

TEST_F(HotWaterStateClassificationTest, HotWaterRunningIsHotWaterState) {
    EXPECT_TRUE(isHotWaterState(MachineStateId::HOT_WATER_RUNNING));
}

TEST_F(HotWaterStateClassificationTest, HotWaterStoppedIsHotWaterState) {
    EXPECT_TRUE(isHotWaterState(MachineStateId::HOT_WATER_STOPPED));
}

TEST_F(HotWaterStateClassificationTest, NonHotWaterStateIsNotHotWaterState) {
    EXPECT_FALSE(isHotWaterState(MachineStateId::INIT));
    EXPECT_FALSE(isHotWaterState(MachineStateId::BREW_IDLE));
    EXPECT_FALSE(isHotWaterState(MachineStateId::STEAM_IDLE));
    EXPECT_FALSE(isHotWaterState(MachineStateId::BACKFLUSH_IDLE));
    EXPECT_FALSE(isHotWaterState(MachineStateId::WATER_TANK_EMPTY));
    EXPECT_FALSE(isHotWaterState(MachineStateId::EMERGENCY_STOP));
}

// ============================================================================
// STEAM STATE CLASSIFICATION TESTS
// ============================================================================

class SteamStateClassificationTest : public ::testing::Test {
   protected:
    void SetUp() override {
        // No setup needed for pure functions
    }
};

TEST_F(SteamStateClassificationTest, SteamIdleIsSteamState) {
    EXPECT_TRUE(isSteamState(MachineStateId::STEAM_IDLE));
}

TEST_F(SteamStateClassificationTest, SteamRunningIsSteamState) {
    EXPECT_TRUE(isSteamState(MachineStateId::STEAM_RUNNING));
}

TEST_F(SteamStateClassificationTest, SteamStoppedIsSteamState) {
    EXPECT_TRUE(isSteamState(MachineStateId::STEAM_STOPPED));
}

TEST_F(SteamStateClassificationTest, NonSteamStateIsNotSteamState) {
    EXPECT_FALSE(isSteamState(MachineStateId::INIT));
    EXPECT_FALSE(isSteamState(MachineStateId::BREW_IDLE));
    EXPECT_FALSE(isSteamState(MachineStateId::HOT_WATER_IDLE));
    EXPECT_FALSE(isSteamState(MachineStateId::BACKFLUSH_IDLE));
    EXPECT_FALSE(isSteamState(MachineStateId::WATER_TANK_EMPTY));
    EXPECT_FALSE(isSteamState(MachineStateId::EMERGENCY_STOP));
}

// ============================================================================
// BACKFLUSH STATE CLASSIFICATION TESTS
// ============================================================================

class BackflushStateClassificationTest : public ::testing::Test {
   protected:
    void SetUp() override {
        // No setup needed for pure functions
    }
};

TEST_F(BackflushStateClassificationTest, BackflushIdleIsBackflushState) {
    EXPECT_TRUE(isBackflushState(MachineStateId::BACKFLUSH_IDLE));
}

TEST_F(BackflushStateClassificationTest, BackflushFillingIsBackflushState) {
    EXPECT_TRUE(isBackflushState(MachineStateId::BACKFLUSH_FILLING));
}

TEST_F(BackflushStateClassificationTest, BackflushFlushingIsBackflushState) {
    EXPECT_TRUE(isBackflushState(MachineStateId::BACKFLUSH_FLUSHING));
}

TEST_F(BackflushStateClassificationTest, BackflushFinishedIsBackflushState) {
    EXPECT_TRUE(isBackflushState(MachineStateId::BACKFLUSH_FINISHED));
}

TEST_F(BackflushStateClassificationTest, NonBackflushStateIsNotBackflushState) {
    EXPECT_FALSE(isBackflushState(MachineStateId::INIT));
    EXPECT_FALSE(isBackflushState(MachineStateId::BREW_IDLE));
    EXPECT_FALSE(isBackflushState(MachineStateId::HOT_WATER_IDLE));
    EXPECT_FALSE(isBackflushState(MachineStateId::STEAM_IDLE));
    EXPECT_FALSE(isBackflushState(MachineStateId::WATER_TANK_EMPTY));
    EXPECT_FALSE(isBackflushState(MachineStateId::EMERGENCY_STOP));
}

// ============================================================================
// MANUAL FLUSH STATE CLASSIFICATION TESTS
// ============================================================================

class ManualFlushStateClassificationTest : public ::testing::Test {
   protected:
    void SetUp() override {
        // No setup needed for pure functions
    }
};

TEST_F(ManualFlushStateClassificationTest, ManualFlushIdleIsManualFlushState) {
    EXPECT_TRUE(isManualFlushState(MachineStateId::MANUAL_FLUSH_IDLE));
}

TEST_F(ManualFlushStateClassificationTest, ManualFlushRunningIsManualFlushState) {
    EXPECT_TRUE(isManualFlushState(MachineStateId::MANUAL_FLUSH_RUNNING));
}

TEST_F(ManualFlushStateClassificationTest, NonManualFlushStateIsNotManualFlushState) {
    EXPECT_FALSE(isManualFlushState(MachineStateId::INIT));
    EXPECT_FALSE(isManualFlushState(MachineStateId::BREW_IDLE));
    EXPECT_FALSE(isManualFlushState(MachineStateId::HOT_WATER_IDLE));
    EXPECT_FALSE(isManualFlushState(MachineStateId::STEAM_IDLE));
    EXPECT_FALSE(isManualFlushState(MachineStateId::BACKFLUSH_IDLE));
    EXPECT_FALSE(isManualFlushState(MachineStateId::WATER_TANK_EMPTY));
    EXPECT_FALSE(isManualFlushState(MachineStateId::EMERGENCY_STOP));
}

// ============================================================================
// CROSS-CATEGORY TESTS (Mutual Exclusivity)
// ============================================================================

class StateClassificationMutualExclusivityTest : public ::testing::Test {
   protected:
    void SetUp() override {
        // No setup needed for pure functions
    }
};

TEST_F(StateClassificationMutualExclusivityTest, BrewStatesAreNotHotWaterStates) {
    EXPECT_FALSE(isHotWaterState(MachineStateId::BREW_IDLE));
    EXPECT_FALSE(isHotWaterState(MachineStateId::BREW_RUNNING));
    EXPECT_FALSE(isHotWaterState(MachineStateId::BREW_FINISHED));
}

TEST_F(StateClassificationMutualExclusivityTest, BrewStatesAreNotSteamStates) {
    EXPECT_FALSE(isSteamState(MachineStateId::BREW_IDLE));
    EXPECT_FALSE(isSteamState(MachineStateId::BREW_RUNNING));
    EXPECT_FALSE(isSteamState(MachineStateId::BREW_FINISHED));
}

TEST_F(StateClassificationMutualExclusivityTest, HotWaterStatesAreNotBrewStates) {
    EXPECT_FALSE(isBrewState(MachineStateId::HOT_WATER_IDLE));
    EXPECT_FALSE(isBrewState(MachineStateId::HOT_WATER_RUNNING));
    EXPECT_FALSE(isBrewState(MachineStateId::HOT_WATER_STOPPED));
}

TEST_F(StateClassificationMutualExclusivityTest, SteamStatesAreNotBrewStates) {
    EXPECT_FALSE(isBrewState(MachineStateId::STEAM_IDLE));
    EXPECT_FALSE(isBrewState(MachineStateId::STEAM_RUNNING));
    EXPECT_FALSE(isBrewState(MachineStateId::STEAM_STOPPED));
}

TEST_F(StateClassificationMutualExclusivityTest, BackflushStatesAreNotBrewStates) {
    EXPECT_FALSE(isBrewState(MachineStateId::BACKFLUSH_IDLE));
    EXPECT_FALSE(isBrewState(MachineStateId::BACKFLUSH_FILLING));
    EXPECT_FALSE(isBrewState(MachineStateId::BACKFLUSH_FLUSHING));
    EXPECT_FALSE(isBrewState(MachineStateId::BACKFLUSH_FINISHED));
}

// ============================================================================
// ERROR AND SPECIAL STATE TESTS
// ============================================================================

class SpecialStateClassificationTest : public ::testing::Test {
   protected:
    void SetUp() override {
        // No setup needed for pure functions
    }
};

TEST_F(SpecialStateClassificationTest, ErrorStatesAreNotBrewStates) {
    EXPECT_FALSE(isBrewState(MachineStateId::WATER_TANK_EMPTY));
    EXPECT_FALSE(isBrewState(MachineStateId::SENSOR_ERROR));
    EXPECT_FALSE(isBrewState(MachineStateId::EEPROM_ERROR));
}

TEST_F(SpecialStateClassificationTest, EmergencyStopIsNotAnyCategory) {
    EXPECT_FALSE(isBrewState(MachineStateId::EMERGENCY_STOP));
    EXPECT_FALSE(isHotWaterState(MachineStateId::EMERGENCY_STOP));
    EXPECT_FALSE(isSteamState(MachineStateId::EMERGENCY_STOP));
    EXPECT_FALSE(isBackflushState(MachineStateId::EMERGENCY_STOP));
    EXPECT_FALSE(isManualFlushState(MachineStateId::EMERGENCY_STOP));
}

TEST_F(SpecialStateClassificationTest, StandbyStateIsNotAnyCategory) {
    EXPECT_FALSE(isBrewState(MachineStateId::STANDBY));
    EXPECT_FALSE(isHotWaterState(MachineStateId::STANDBY));
    EXPECT_FALSE(isSteamState(MachineStateId::STANDBY));
    EXPECT_FALSE(isBackflushState(MachineStateId::STANDBY));
    EXPECT_FALSE(isManualFlushState(MachineStateId::STANDBY));
}

// ============================================================================
// COMPREHENSIVE COVERAGE TESTS
// ============================================================================

class ComprehensiveStateClassificationTest : public ::testing::Test {
   protected:
    void SetUp() override {
        // No setup needed for pure functions
    }
};

TEST_F(ComprehensiveStateClassificationTest, AllBrewStatesCovered) {
    // Verify all brew states are classified correctly
    std::array<MachineStateId, 5> brewStates = {
        MachineStateId::BREW_IDLE,
        MachineStateId::BREW_PREINFUSION,
        MachineStateId::BREW_PREINFUSION_PAUSE,
        MachineStateId::BREW_RUNNING,
        MachineStateId::BREW_FINISHED,
    };

    for (const auto& state : brewStates) {
        EXPECT_TRUE(isBrewState(state)) << "State " << static_cast<int>(state) << " should be a brew state";
    }
}

TEST_F(ComprehensiveStateClassificationTest, AllHotWaterStatesCovered) {
    // Verify all hot water states are classified correctly
    std::array<MachineStateId, 3> hotWaterStates = {
        MachineStateId::HOT_WATER_IDLE,
        MachineStateId::HOT_WATER_RUNNING,
        MachineStateId::HOT_WATER_STOPPED,
    };

    for (const auto& state : hotWaterStates) {
        EXPECT_TRUE(isHotWaterState(state)) << "State " << static_cast<int>(state)
                                             << " should be a hot water state";
    }
}

TEST_F(ComprehensiveStateClassificationTest, AllSteamStatesCovered) {
    // Verify all steam states are classified correctly
    std::array<MachineStateId, 3> steamStates = {
        MachineStateId::STEAM_IDLE,
        MachineStateId::STEAM_RUNNING,
        MachineStateId::STEAM_STOPPED,
    };

    for (const auto& state : steamStates) {
        EXPECT_TRUE(isSteamState(state)) << "State " << static_cast<int>(state) << " should be a steam state";
    }
}

TEST_F(ComprehensiveStateClassificationTest, AllBackflushStatesCovered) {
    // Verify all backflush states are classified correctly
    std::array<MachineStateId, 4> backflushStates = {
        MachineStateId::BACKFLUSH_IDLE,
        MachineStateId::BACKFLUSH_FILLING,
        MachineStateId::BACKFLUSH_FLUSHING,
        MachineStateId::BACKFLUSH_FINISHED,
    };

    for (const auto& state : backflushStates) {
        EXPECT_TRUE(isBackflushState(state)) << "State " << static_cast<int>(state)
                                              << " should be a backflush state";
    }
}

TEST_F(ComprehensiveStateClassificationTest, AllManualFlushStatesCovered) {
    // Verify all manual flush states are classified correctly
    std::array<MachineStateId, 2> manualFlushStates = {
        MachineStateId::MANUAL_FLUSH_IDLE,
        MachineStateId::MANUAL_FLUSH_RUNNING,
    };

    for (const auto& state : manualFlushStates) {
        EXPECT_TRUE(isManualFlushState(state)) << "State " << static_cast<int>(state)
                                                << " should be a manual flush state";
    }
}
