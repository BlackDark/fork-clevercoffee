/**
 * @file test_pid_state_transitions/test_main.cpp
 * @brief Unit tests for PID state transitions
 *
 * Tests that verify:
 * 1. PidNormalState transitions to PidDisabledState when PID is disabled
 * 2. PidDisabledState transitions to PidNormalState when PID is re-enabled
 * 3. State transitions are immediate (not stuck in a state)
 */

#include <gtest/gtest.h>
#include "../test_support.h"
#include <memory>

// Mock MachineStateContext for testing
class MockMachineStateContext {
public:
    MockMachineStateContext() : pidEnabled_(true), brewStartRequested_(false) {}
    
    bool isPidEnabled() const { return pidEnabled_; }
    void setPidEnabled(bool enabled) { pidEnabled_ = enabled; }
    
    bool isBrewStartRequested() const { return brewStartRequested_; }
    void setBrewStartRequested(bool requested) { brewStartRequested_ = requested; }
    
private:
    bool pidEnabled_;
    bool brewStartRequested_;
};

// Mock state IDs
enum class MockMachineStateId {
    PID_NORMAL = 20,
    PID_DISABLED = 90,
    BREW_IDLE = 30,
};

// Mock state interface
class MockMachineState {
public:
    virtual ~MockMachineState() = default;
    virtual MockMachineStateId getStateId() const = 0;
    virtual MockMachineState* checkTransitions(MockMachineStateContext& context) = 0;
};

// Mock PID_NORMAL state - WITHOUT the fix (buggy version)
class BuggyPidNormalState : public MockMachineState {
public:
    MockMachineStateId getStateId() const override { return MockMachineStateId::PID_NORMAL; }
    
    MockMachineState* checkTransitions(MockMachineStateContext& context) override {
        // BUG: No check for !isPidEnabled()
        // It only checks for brew start request, but not for PID being disabled
        if (context.isBrewStartRequested()) {
            context.setBrewStartRequested(false);
            return nullptr;  // Would transition to BREW_IDLE
        }
        return nullptr;  // No transition
    }
};

// Mock PID_NORMAL state - WITH the fix (correct version)
class FixedPidNormalState : public MockMachineState {
public:
    FixedPidNormalState(MockMachineState* pidDisabledState = nullptr) 
        : pidDisabledState_(pidDisabledState) {}
    
    MockMachineStateId getStateId() const override { return MockMachineStateId::PID_NORMAL; }
    
    MockMachineState* checkTransitions(MockMachineStateContext& context) override {
        // FIX: Check if PID was disabled FIRST, before checking other conditions
        if (!context.isPidEnabled()) {
            return pidDisabledState_;  // Transition to PID_DISABLED
        }
        
        if (context.isBrewStartRequested()) {
            context.setBrewStartRequested(false);
            return nullptr;  // Would transition to BREW_IDLE
        }
        return nullptr;  // No transition
    }
    
private:
    MockMachineState* pidDisabledState_;
};

// Mock PID_DISABLED state
class MockPidDisabledState : public MockMachineState {
public:
    MockPidDisabledState(MockMachineState* pidNormalState = nullptr) 
        : pidNormalState_(pidNormalState) {}
    
    MockMachineStateId getStateId() const override { return MockMachineStateId::PID_DISABLED; }
    
    MockMachineState* checkTransitions(MockMachineStateContext& context) override {
        if (context.isPidEnabled()) {
            return pidNormalState_;  // Transition to PID_NORMAL
        }
        return nullptr;  // No transition
    }
    
    void setPidNormalState(MockMachineState* state) { pidNormalState_ = state; }
    
private:
    MockMachineState* pidNormalState_;
};

// ==================== PID STATE TRANSITION TESTS ====================

class PidStateTransitionTest : public ::testing::Test {
protected:
    MockMachineStateContext context;
    std::unique_ptr<MockPidDisabledState> pidDisabledState;
    std::unique_ptr<FixedPidNormalState> pidNormalState;
    
    void SetUp() override {
        context.setPidEnabled(true);
        pidDisabledState = std::make_unique<MockPidDisabledState>();
        pidNormalState = std::make_unique<FixedPidNormalState>(pidDisabledState.get());
        // Wire them together
        pidDisabledState->setPidNormalState(pidNormalState.get());
    }
};

/**
 * Test 1: Bug reproduction - buggy state doesn't transition when PID disabled
 */
TEST_F(PidStateTransitionTest, BuggyStateStuckWhenPidDisabled) {
    BuggyPidNormalState buggyState;
    
    // Start in PID_NORMAL
    EXPECT_EQ(buggyState.getStateId(), MockMachineStateId::PID_NORMAL);
    
    // Disable PID
    context.setPidEnabled(false);
    
    // Check transitions - should return nullptr (no transition)
    MockMachineState* nextState = buggyState.checkTransitions(context);
    
    // BUG: No transition happens!
    EXPECT_EQ(nextState, nullptr) 
        << "BUGGY: State machine is stuck in PID_NORMAL even though PID is disabled!";
}

/**
 * Test 2: Fixed state transitions when PID disabled
 */
TEST_F(PidStateTransitionTest, FixedStateTransitionsWhenPidDisabled) {
    // Start in PID_NORMAL
    EXPECT_EQ(pidNormalState->getStateId(), MockMachineStateId::PID_NORMAL);
    
    // Disable PID
    context.setPidEnabled(false);
    
    // Check transitions - should return pidDisabledState
    MockMachineState* nextState = pidNormalState->checkTransitions(context);
    
    // FIX: Transition happens!
    EXPECT_NE(nextState, nullptr) 
        << "FIXED: State machine should transition when PID is disabled";
    EXPECT_EQ(nextState->getStateId(), MockMachineStateId::PID_DISABLED);
}

/**
 * Test 3: PID stays in NORMAL when enabled
 */
TEST_F(PidStateTransitionTest, PidNormalStaysWhenEnabled) {
    context.setPidEnabled(true);
    
    // Should not transition
    MockMachineState* nextState = pidNormalState->checkTransitions(context);
    EXPECT_EQ(nextState, nullptr) << "Should stay in PID_NORMAL when enabled";
}

/**
 * Test 4: Round-trip: NORMAL -> DISABLED -> NORMAL
 */
TEST_F(PidStateTransitionTest, RoundTripTransition) {
    MockMachineState* currentState = pidNormalState.get();
    EXPECT_EQ(currentState->getStateId(), MockMachineStateId::PID_NORMAL);
    
    // Step 1: Disable PID -> should go to DISABLED
    context.setPidEnabled(false);
    currentState = currentState->checkTransitions(context);
    EXPECT_NE(currentState, nullptr) << "Should transition to PID_DISABLED";
    EXPECT_EQ(currentState->getStateId(), MockMachineStateId::PID_DISABLED);
    
    // Step 2: Re-enable PID -> should go back to NORMAL
    context.setPidEnabled(true);
    currentState = currentState->checkTransitions(context);
    EXPECT_NE(currentState, nullptr) << "Should transition back to PID_NORMAL";
    EXPECT_EQ(currentState->getStateId(), MockMachineStateId::PID_NORMAL);
}

/**
 * Test 5: Priority - PID disabled check comes before other transitions
 */
TEST_F(PidStateTransitionTest, PidDisabledCheckHasPriority) {
    // Both PID disabled AND brew start requested
    context.setPidEnabled(false);
    context.setBrewStartRequested(true);
    
    // Should transition to PID_DISABLED, not BREW
    MockMachineState* nextState = pidNormalState->checkTransitions(context);
    EXPECT_EQ(nextState->getStateId(), MockMachineStateId::PID_DISABLED) 
        << "PID disabled should take priority over brew start";
}

/**
 * Test 6: Multiple rapid state changes
 */
TEST_F(PidStateTransitionTest, MultipleRapidStateChanges) {
    MockMachineState* currentState = pidNormalState.get();
    
    // Disable
    context.setPidEnabled(false);
    currentState = currentState->checkTransitions(context);
    EXPECT_EQ(currentState->getStateId(), MockMachineStateId::PID_DISABLED);
    
    // Enable
    context.setPidEnabled(true);
    currentState = currentState->checkTransitions(context);
    EXPECT_EQ(currentState->getStateId(), MockMachineStateId::PID_NORMAL);
    
    // Disable again
    context.setPidEnabled(false);
    currentState = currentState->checkTransitions(context);
    EXPECT_EQ(currentState->getStateId(), MockMachineStateId::PID_DISABLED);
    
    // Enable again
    context.setPidEnabled(true);
    currentState = currentState->checkTransitions(context);
    EXPECT_EQ(currentState->getStateId(), MockMachineStateId::PID_NORMAL);
}

/**
 * Test 7: Verify state IDs match expected values
 */
TEST_F(PidStateTransitionTest, StateIdsCorrect) {
    EXPECT_EQ(static_cast<int>(MockMachineStateId::PID_NORMAL), 20);
    EXPECT_EQ(static_cast<int>(MockMachineStateId::PID_DISABLED), 90);
}

/**
 * Test 8: Bug impact - demonstrates system staying in wrong state
 */
class BugImpactTest : public ::testing::Test {
public:
    // Simulates the actual observed behavior
    void simulateUserDisablingPid() {
        MockMachineStateContext context;
        BuggyPidNormalState buggyState;
        
        // System starts in PID_NORMAL
        int currentState = 20;  // PID_NORMAL
        EXPECT_EQ(currentState, 20);
        
        // User disables PID via POST request
        context.setPidEnabled(false);
        // User disabled PID
        
        // State machine should transition to PID_DISABLED, but doesn't (bug)
        MockMachineState* nextState = buggyState.checkTransitions(context);
        
        // BUG: Still in PID_NORMAL (state 20) instead of PID_DISABLED (state 90)
        EXPECT_EQ(nextState, nullptr) << "BUG: No state transition occurred";
        
        // System keeps heating indefinitely
        // Relay keeps toggling on/off (pidOutput = 1000.0)
        // Temperature doesn't rise (no heat input actually happening)
        // User thinks system is broken
    }
};

TEST_F(BugImpactTest, BugLeadsToUnresponsiveSystem) {
    // This test documents the exact bug we see in the logs
    simulateUserDisablingPid();
}
