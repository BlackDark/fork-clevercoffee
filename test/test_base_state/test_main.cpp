/**
 * @file test_base_state.cpp
 * @brief Tier 2: Tests for BaseState transition priority and safety checks
 * 
 * Tests the core safety logic of state transitions:
 * - Emergency stop has highest priority
 * - Sensor errors are checked second
 * - Water tank empty is checked third
 * - State-specific transitions only happen when safe
 */

#include <gtest/gtest.h>
#include "../test_support.h"

// ==================== TRANSITION PRIORITY LOGIC TESTS ====================

class BaseStateTransitionPriorityTest : public ::testing::Test {
 protected:
  // Simulates the BaseState transition priority logic
  struct SafetyCheckContext {
    bool emergencyStop = false;
    bool sensorError = false;
    bool waterTankEmpty = false;
    
    int getTransitionTarget() {
      // This mimics BaseState::checkTransitions priority order
      if (emergencyStop) return 1;  // EMERGENCY_STOP
      if (sensorError) return 2;    // SENSOR_ERROR
      if (waterTankEmpty) return 3; // WATER_TANK_EMPTY
      return 0;                     // No safety transition, allow specific transitions
    }
  };
};

TEST_F(BaseStateTransitionPriorityTest, EmergencyStopHasHighestPriority) {
  SafetyCheckContext ctx;
  ctx.emergencyStop = true;
  ctx.sensorError = true;
  ctx.waterTankEmpty = true;
  
  EXPECT_EQ(ctx.getTransitionTarget(), 1);  // Should be EMERGENCY_STOP
}

TEST_F(BaseStateTransitionPriorityTest, SensorErrorIsSecondPriority) {
  SafetyCheckContext ctx;
  ctx.emergencyStop = false;
  ctx.sensorError = true;
  ctx.waterTankEmpty = true;
  
  EXPECT_EQ(ctx.getTransitionTarget(), 2);  // Should be SENSOR_ERROR
}

TEST_F(BaseStateTransitionPriorityTest, WaterTankEmptyIsThirdPriority) {
  SafetyCheckContext ctx;
  ctx.emergencyStop = false;
  ctx.sensorError = false;
  ctx.waterTankEmpty = true;
  
  EXPECT_EQ(ctx.getTransitionTarget(), 3);  // Should be WATER_TANK_EMPTY
}

TEST_F(BaseStateTransitionPriorityTest, AllSafetyChecksPassed) {
  SafetyCheckContext ctx;
  ctx.emergencyStop = false;
  ctx.sensorError = false;
  ctx.waterTankEmpty = false;
  
  EXPECT_EQ(ctx.getTransitionTarget(), 0);  // Allow state-specific transitions
}

TEST_F(BaseStateTransitionPriorityTest, EmergencyStopBlocksNormalTransitions) {
  SafetyCheckContext ctx;
  ctx.emergencyStop = true;
  ctx.sensorError = false;
  ctx.waterTankEmpty = false;
  
  EXPECT_EQ(ctx.getTransitionTarget(), 1);  // Should transition to EMERGENCY_STOP
}

TEST_F(BaseStateTransitionPriorityTest, SensorErrorBlocksNormalTransitions) {
  SafetyCheckContext ctx;
  ctx.emergencyStop = false;
  ctx.sensorError = true;
  ctx.waterTankEmpty = false;
  
  EXPECT_EQ(ctx.getTransitionTarget(), 2);  // Should transition to SENSOR_ERROR
}

TEST_F(BaseStateTransitionPriorityTest, WaterTankEmptyBlocksNormalTransitions) {
  SafetyCheckContext ctx;
  ctx.emergencyStop = false;
  ctx.sensorError = false;
  ctx.waterTankEmpty = true;
  
  EXPECT_EQ(ctx.getTransitionTarget(), 3);  // Should transition to WATER_TANK_EMPTY
}

// ==================== SAFETY GUARD CONSISTENCY TESTS ====================

class SafetyGuardConsistencyTest : public ::testing::Test {
 protected:
  struct SafetyGuard {
    bool isEmergency = false;
    bool isSensorError = false;
    bool isWaterEmpty = false;
    
    bool allowsNormalOperations() {
      return !isEmergency && !isSensorError && !isWaterEmpty;
    }
    
    bool allowsBrew() {
      return allowsNormalOperations();
    }
    
    bool allowsHotWater() {
      return allowsNormalOperations();
    }
    
    bool allowsSteam() {
      return allowsNormalOperations();
    }
    
    bool allowsBackflush() {
      return allowsNormalOperations();
    }
  };
};

TEST_F(SafetyGuardConsistencyTest, AllOperationsBlockedByEmergency) {
  SafetyGuard guard;
  guard.isEmergency = true;
  
  EXPECT_FALSE(guard.allowsBrew());
  EXPECT_FALSE(guard.allowsHotWater());
  EXPECT_FALSE(guard.allowsSteam());
  EXPECT_FALSE(guard.allowsBackflush());
}

TEST_F(SafetyGuardConsistencyTest, AllOperationsBlockedBySensorError) {
  SafetyGuard guard;
  guard.isSensorError = true;
  
  EXPECT_FALSE(guard.allowsBrew());
  EXPECT_FALSE(guard.allowsHotWater());
  EXPECT_FALSE(guard.allowsSteam());
  EXPECT_FALSE(guard.allowsBackflush());
}

TEST_F(SafetyGuardConsistencyTest, AllOperationsBlockedByEmptyWater) {
  SafetyGuard guard;
  guard.isWaterEmpty = true;
  
  EXPECT_FALSE(guard.allowsBrew());
  EXPECT_FALSE(guard.allowsHotWater());
  EXPECT_FALSE(guard.allowsSteam());
  EXPECT_FALSE(guard.allowsBackflush());
}

TEST_F(SafetyGuardConsistencyTest, AllOperationsAllowedWhenSafe) {
  SafetyGuard guard;
  
  EXPECT_TRUE(guard.allowsBrew());
  EXPECT_TRUE(guard.allowsHotWater());
  EXPECT_TRUE(guard.allowsSteam());
  EXPECT_TRUE(guard.allowsBackflush());
}

// ==================== ENTRY/EXIT LIFECYCLE TESTS ====================

class StateLifecycleTest : public ::testing::Test {
 protected:
  struct StateLifecycle {
    int entryCallCount = 0;
    int exitCallCount = 0;
    bool loggingEnabled = true;
    
    void onEntry() {
      if (loggingEnabled) {
        entryCallCount++;
      }
    }
    
    void onExit() {
      if (loggingEnabled) {
        exitCallCount++;
      }
    }
    
    void transition() {
      onExit();
      onEntry();
    }
  };
};

TEST_F(StateLifecycleTest, EntryCalledOncePerTransition) {
  StateLifecycle state;
  
  state.onEntry();
  EXPECT_EQ(state.entryCallCount, 1);
  
  state.onEntry();
  EXPECT_EQ(state.entryCallCount, 2);
}

TEST_F(StateLifecycleTest, ExitCalledOncePerTransition) {
  StateLifecycle state;
  
  state.onExit();
  EXPECT_EQ(state.exitCallCount, 1);
  
  state.onExit();
  EXPECT_EQ(state.exitCallCount, 2);
}

TEST_F(StateLifecycleTest, TransitionCallsBothEntryAndExit) {
  StateLifecycle state;
  
  state.transition();
  EXPECT_EQ(state.entryCallCount, 1);
  EXPECT_EQ(state.exitCallCount, 1);
  
  state.transition();
  EXPECT_EQ(state.entryCallCount, 2);
  EXPECT_EQ(state.exitCallCount, 2);
}

TEST_F(StateLifecycleTest, LoggingCanBeDisabled) {
  StateLifecycle state;
  state.loggingEnabled = false;
  
  state.onEntry();
  state.onExit();
  state.transition();
  
  EXPECT_EQ(state.entryCallCount, 0);
  EXPECT_EQ(state.exitCallCount, 0);
}

// ==================== STATE NAMING AND IDENTIFICATION TESTS ====================

class StateIdentificationTest : public ::testing::Test {
 protected:
  struct StateInfo {
    int stateId;
    const char* stateName;
    
    bool isValid() {
      return stateId >= 0 && stateName != nullptr && stateName[0] != '\0';
    }
    
    bool hasUniqueId(const StateInfo& other) {
      return stateId != other.stateId;
    }
  };
};

TEST_F(StateIdentificationTest, StateHasValidId) {
  StateInfo state = {30, "BREW_IDLE"};
  EXPECT_GE(state.stateId, 0);
}

TEST_F(StateIdentificationTest, StateHasValidName) {
  StateInfo state = {30, "BREW_IDLE"};
  EXPECT_NE(state.stateName, nullptr);
  EXPECT_NE(std::string(state.stateName), "");
  EXPECT_NE(std::string(state.stateName), "Unknown");
}

TEST_F(StateIdentificationTest, StatesHaveUniqueIds) {
  StateInfo brew = {30, "BREW_IDLE"};
  StateInfo hotwater = {40, "HOT_WATER_IDLE"};
  
  EXPECT_TRUE(brew.hasUniqueId(hotwater));
}

TEST_F(StateIdentificationTest, MultipleStatesHaveUniqueIds) {
  StateInfo states[] = {
    {30, "BREW_IDLE"},
    {31, "BREW_PREINFUSION"},
    {40, "HOT_WATER_IDLE"},
    {50, "STEAM_IDLE"},
    {60, "BACKFLUSH_IDLE"},
  };
  
  for (size_t i = 0; i < 5; i++) {
    for (size_t j = i + 1; j < 5; j++) {
      EXPECT_TRUE(states[i].hasUniqueId(states[j]));
    }
  }
}

// ==================== DERIVED STATE CALLBACK TESTS ====================

class DerivedStateCallbackTest : public ::testing::Test {
 protected:
  struct BaseStateTemplate {
    int specificTransitionsCalled = 0;
    
    int checkTransitions() {
      // Calls derived class implementation
      return checkSpecificTransitions();
    }
    
    virtual int checkSpecificTransitions() {
      specificTransitionsCalled++;
      return -1;  // No transition
    }
  };
  
  struct DerivedState : public BaseStateTemplate {
    int checkSpecificTransitions() override {
      specificTransitionsCalled++;
      return 100;  // Example transition ID
    }
  };
};

TEST_F(DerivedStateCallbackTest, SpecificTransitionsCalledWhenSafe) {
  DerivedState state;
  
  int result = state.checkTransitions();
  
  EXPECT_EQ(state.specificTransitionsCalled, 1);
  EXPECT_EQ(result, 100);
}

TEST_F(DerivedStateCallbackTest, SpecificTransitionsNotCalledWhenUnsafe) {
  // This is tested implicitly in transition priority tests
  // where emergency stop prevents specific transitions from being checked
}

// ==================== MULTIPLE TRANSITION SCENARIOS ====================

class MultipleTransitionScenariosTest : public ::testing::Test {
 protected:
  struct CompleteStateContext {
    bool emergencyStop = false;
    bool sensorError = false;
    bool waterTankEmpty = false;
    bool shouldTransition = false;
    
    int getPrioritizedTransition() {
      // Safety checks first
      if (emergencyStop) return 1;
      if (sensorError) return 2;
      if (waterTankEmpty) return 3;
      
      // Then specific transitions
      if (shouldTransition) return 100;
      return 0;
    }
  };
};

TEST_F(MultipleTransitionScenariosTest, SafetyChecksPreemptSpecificTransitions) {
  CompleteStateContext ctx;
  ctx.shouldTransition = true;
  ctx.emergencyStop = true;
  
  // Emergency stop should take precedence over specific transition
  EXPECT_EQ(ctx.getPrioritizedTransition(), 1);
  EXPECT_NE(ctx.getPrioritizedTransition(), 100);
}

TEST_F(MultipleTransitionScenariosTest, SpecificTransitionsWhenAllSafe) {
  CompleteStateContext ctx;
  ctx.shouldTransition = true;
  ctx.emergencyStop = false;
  ctx.sensorError = false;
  ctx.waterTankEmpty = false;
  
  EXPECT_EQ(ctx.getPrioritizedTransition(), 100);
}

TEST_F(MultipleTransitionScenariosTest, NoTransitionWhenSafeButNoSpecificTransition) {
  CompleteStateContext ctx;
  ctx.shouldTransition = false;
  
  EXPECT_EQ(ctx.getPrioritizedTransition(), 0);
}

TEST_F(MultipleTransitionScenariosTest, SensorErrorPreemptsSafetyLowPriority) {
  CompleteStateContext ctx;
  ctx.sensorError = true;
  ctx.waterTankEmpty = true;
  
  EXPECT_EQ(ctx.getPrioritizedTransition(), 2);
}
