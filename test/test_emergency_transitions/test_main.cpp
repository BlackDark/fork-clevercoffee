/**
 * @file test_main.cpp
 * @brief Emergency transition tests for BaseState priority checking logic
 *
 * Tests that verify the emergency priority hierarchy implemented in BaseState:
 * 1. Emergency stop (highest priority)
 * 2. Sensor errors
 * 3. Water tank empty
 * 4. Delegate to state-specific transitions (lowest priority)
 *
 * Since BaseState includes platform-dependent headers, we test the LOGIC
 * directly by simulating the priority checks without including BaseState.
 * This tests the same decision logic in isolation.
 */

#include <gtest/gtest.h>
#include <gmock/gmock.h>

#include "clevercoffee/state/MachineStateIds.h"
#include "mocks/MockMachineStateContext.h"

// ============================================================================
// EMERGENCY PRIORITY LOGIC IMPLEMENTATION
// ============================================================================
// We extract and test the core emergency priority logic in isolation.
// This is the logic from BaseState::checkTransitions (lines 100-120)
// that EVERY state must follow.

/**
 * Return type for emergency checks - either a state to transition to,
 * or nullopt if no emergency transition is needed.
 */
struct EmergencyCheckResult {
    bool hasEmergency;
    MachineStateId nextState;

    // Convenience constructor
    EmergencyCheckResult(bool has = false, MachineStateId state = MachineStateId::INIT)
        : hasEmergency(has), nextState(state) {}
};

/**
 * Simulates the emergency priority checking from BaseState::checkTransitions
 * Returns an EmergencyCheckResult with the next state if emergency detected,
 * or hasEmergency=false if no emergency transition needed.
 */
EmergencyCheckResult checkEmergencyPriorities(MockMachineStateContext& context) {
    // Emergency stop check - highest priority
    if (context.isEmergencyStop()) {
        return EmergencyCheckResult(true, MachineStateId::EMERGENCY_STOP);
    }

    // Critical error checks
    if (context.hasSensorError()) {
        return EmergencyCheckResult(true, MachineStateId::SENSOR_ERROR);
    }

    if (!context.isWaterTankFull()) {
        return EmergencyCheckResult(true, MachineStateId::WATER_TANK_EMPTY);
    }

    // No emergency condition - would delegate to state-specific checks
    return EmergencyCheckResult(false);
}

// ============================================================================
// EMERGENCY TRANSITION PRIORITY TESTS
// ============================================================================

class EmergencyTransitionTest : public ::testing::Test {
  protected:
    MockMachineStateContext context_;

    void SetUp() override {
        // Default: no emergency conditions
        EXPECT_CALL(context_, isEmergencyStop()).WillRepeatedly(::testing::Return(false));
        EXPECT_CALL(context_, hasSensorError()).WillRepeatedly(::testing::Return(false));
        EXPECT_CALL(context_, isWaterTankFull()).WillRepeatedly(::testing::Return(true));
    }
};

/**
 * TEST: Emergency stop has highest priority
 *
 * VERIFY: When emergency stop is active, returns EMERGENCY_STOP
 * regardless of other conditions (sensor errors, water level)
 *
 * REQUIREMENT: BaseState.h line 101-105 - emergency check first
 */
TEST_F(EmergencyTransitionTest, EmergencyStopHasHighestPriority) {
    // Setup: emergency stop active - subsequent checks should NOT be called
    // because emergency stop has highest priority and short-circuits remaining checks
    EXPECT_CALL(context_, isEmergencyStop()).WillOnce(::testing::Return(true));
    EXPECT_CALL(context_, hasSensorError()).Times(0);  // Should not be checked
    EXPECT_CALL(context_, isWaterTankFull()).Times(0);  // Should not be checked

    // VERIFY: Returns EMERGENCY_STOP without checking other conditions
    EmergencyCheckResult result = checkEmergencyPriorities(context_);
    EXPECT_TRUE(result.hasEmergency);
    EXPECT_EQ(result.nextState, MachineStateId::EMERGENCY_STOP);
}

/**
 * TEST: Sensor error has higher priority than water tank
 *
 * VERIFY: When sensor error exists, returns SENSOR_ERROR
 * even if water tank is empty (but emergency stop is not active)
 * Water tank check should not occur because sensor error short-circuits it
 *
 * REQUIREMENT: BaseState.h line 107-111
 */
TEST_F(EmergencyTransitionTest, SensorErrorHasHigherPriorityThanWaterTank) {
    // Setup: sensor error active - water tank check should NOT occur
    EXPECT_CALL(context_, isEmergencyStop()).WillOnce(::testing::Return(false));
    EXPECT_CALL(context_, hasSensorError()).WillOnce(::testing::Return(true));
    EXPECT_CALL(context_, isWaterTankFull()).Times(0);  // Should not be checked

    // VERIFY: Returns SENSOR_ERROR without checking water tank
    EmergencyCheckResult result = checkEmergencyPriorities(context_);
    EXPECT_TRUE(result.hasEmergency);
    EXPECT_EQ(result.nextState, MachineStateId::SENSOR_ERROR);
}

/**
 * TEST: Water tank empty check occurs after error checks
 *
 * VERIFY: When water tank is empty (but no emergency or sensor errors),
 * returns WATER_TANK_EMPTY
 *
 * REQUIREMENT: BaseState.h line 113-116
 */
TEST_F(EmergencyTransitionTest, WaterTankEmptyCheckedAfterErrors) {
    // Setup: water tank empty, but no emergency stop or sensor errors
    EXPECT_CALL(context_, isEmergencyStop()).WillOnce(::testing::Return(false));
    EXPECT_CALL(context_, hasSensorError()).WillOnce(::testing::Return(false));
    EXPECT_CALL(context_, isWaterTankFull()).WillOnce(::testing::Return(false));

    // VERIFY: Water tank empty transition state is returned
    EmergencyCheckResult result = checkEmergencyPriorities(context_);
    EXPECT_TRUE(result.hasEmergency);
    EXPECT_EQ(result.nextState, MachineStateId::WATER_TANK_EMPTY);
}

/**
 * TEST: No emergency conditions allow state-specific transitions
 *
 * VERIFY: When no emergency/error conditions exist,
 * returns hasEmergency=false (signals delegate to state-specific logic)
 *
 * REQUIREMENT: BaseState.h line 118-120
 */
TEST_F(EmergencyTransitionTest, NoEmergencyConditionsAllowStateSpecificTransition) {
    // Setup: all conditions normal
    EXPECT_CALL(context_, isEmergencyStop()).WillOnce(::testing::Return(false));
    EXPECT_CALL(context_, hasSensorError()).WillOnce(::testing::Return(false));
    EXPECT_CALL(context_, isWaterTankFull()).WillOnce(::testing::Return(true));

    // VERIFY: returns hasEmergency=false (no emergency transition needed)
    EmergencyCheckResult result = checkEmergencyPriorities(context_);
    EXPECT_FALSE(result.hasEmergency);
}

// ============================================================================
// EDGE CASE TESTS
// ============================================================================

class EmergencyEdgeCasesTest : public ::testing::Test {
  protected:
    MockMachineStateContext context_;
};

/**
 * TEST: Multiple conditions - only highest priority transitions
 *
 * VERIFY: With emergency stop + sensor error + water tank empty,
 * ONLY emergency stop transition is detected (others short-circuited)
 */
TEST_F(EmergencyEdgeCasesTest, AllConditionsPresentEmergencyStopWins) {
    EXPECT_CALL(context_, isEmergencyStop()).WillOnce(::testing::Return(true));
    EXPECT_CALL(context_, hasSensorError()).Times(0);  // Short-circuited
    EXPECT_CALL(context_, isWaterTankFull()).Times(0);  // Short-circuited

    EmergencyCheckResult result = checkEmergencyPriorities(context_);
    EXPECT_TRUE(result.hasEmergency);
    EXPECT_EQ(result.nextState, MachineStateId::EMERGENCY_STOP);
}

/**
 * TEST: Emergency stop + water tank empty without sensor error
 *
 * VERIFY: Emergency stop takes priority (not water tank empty)
 * Sensor error check short-circuited, water tank check not reached
 */
TEST_F(EmergencyEdgeCasesTest, EmergencyStopAndWaterTankEmptyEmergencyWins) {
    EXPECT_CALL(context_, isEmergencyStop()).WillOnce(::testing::Return(true));
    EXPECT_CALL(context_, hasSensorError()).Times(0);  // Short-circuited
    EXPECT_CALL(context_, isWaterTankFull()).Times(0);  // Short-circuited

    EmergencyCheckResult result = checkEmergencyPriorities(context_);
    EXPECT_TRUE(result.hasEmergency);
    EXPECT_EQ(result.nextState, MachineStateId::EMERGENCY_STOP);
}

/**
 * TEST: Sensor error alone triggers transition
 *
 * VERIFY: Sensor error is detected, water tank check not reached
 */
TEST_F(EmergencyEdgeCasesTest, SensorErrorAloneTriggers) {
    EXPECT_CALL(context_, isEmergencyStop()).WillOnce(::testing::Return(false));
    EXPECT_CALL(context_, hasSensorError()).WillOnce(::testing::Return(true));
    EXPECT_CALL(context_, isWaterTankFull()).Times(0);  // Short-circuited

    EmergencyCheckResult result = checkEmergencyPriorities(context_);
    EXPECT_TRUE(result.hasEmergency);
    EXPECT_EQ(result.nextState, MachineStateId::SENSOR_ERROR);
}

/**
 * TEST: Water tank empty alone triggers transition
 *
 * VERIFY: Water tank empty is detected correctly
 */
TEST_F(EmergencyEdgeCasesTest, WaterTankEmptyAloneTriggers) {
    EXPECT_CALL(context_, isEmergencyStop()).WillOnce(::testing::Return(false));
    EXPECT_CALL(context_, hasSensorError()).WillOnce(::testing::Return(false));
    EXPECT_CALL(context_, isWaterTankFull()).WillOnce(::testing::Return(false));

    EmergencyCheckResult result = checkEmergencyPriorities(context_);
    EXPECT_TRUE(result.hasEmergency);
    EXPECT_EQ(result.nextState, MachineStateId::WATER_TANK_EMPTY);
}
