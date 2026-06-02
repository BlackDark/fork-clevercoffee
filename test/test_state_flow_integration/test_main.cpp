/**
 * @file test_main.cpp
 * @brief State flow integration tests: full end-to-end state transitions with hardware verification
 *
 * Tests complete state flows through the machine state machine, verifying hardware
 * calls (pump, valve) at each transition point. Uses TestHardwareSpy to track calls.
 */

#include <gtest/gtest.h>

#include <memory>

#include "../ConfigTestHelper.h"
#include "../test_support.h"
#include "../mocks/MockWiFiManager.h"
#include "../mocks/HandlerTestStubs.cpp"

#include "clevercoffee/hardware/HardwareManager.h"
#include "clevercoffee/state/states/BrewStates.h"
#include "clevercoffee/state/states/EmergencyStopState.h"
#include "clevercoffee/state/states/InitState.h"
#include "clevercoffee/state/states/PidStates.h"
#include "clevercoffee/state/states/ErrorStates.h"
#include "clevercoffee/state/states/SystemStates.h"

const char* getStateName(MachineStateId) {
    return "test";
}

#include "../../src/state/states/EmergencyStopState.cpp"
#include "../../src/state/states/InitState.cpp"
#include "../../src/state/states/PidStates.cpp"
#include "../../src/state/states/BrewStates.cpp"
#include "../../src/state/states/ErrorStates.cpp"
#include "../../src/state/states/SystemStates.cpp"

using ::testing::NiceMock;

class StateFlowIntegrationTest : public ::testing::Test {
  protected:
    void SetUp() override {
        CleverCoffee::TestHardwareSpy::reset();
        systemContext_ = std::make_unique<CleverCoffee::SystemContext>();
        systemContext_->markReady();

        dummyHwManager_      = std::make_unique<CleverCoffee::HardwareManager>(Config::getInstance());
        dummyDisplayManager_ = std::make_unique<DisplayManager>(::Hardware::OLEDType::SSD1306,
                                                                ::Hardware::OLEDAddress::ADDR_3C);
        dummyMqttManager_    = std::make_unique<MQTTManager>();
        dummyWiFiManager_    = std::make_unique<NiceMock<MockWiFiManager>>();

        machineStateContext_ = std::make_unique<MachineStateContext>(
            *systemContext_, *dummyHwManager_, *dummyDisplayManager_, *dummyWiFiManager_, *dummyMqttManager_);
        systemContext_->setMachineStateContext(machineStateContext_.get());
    }

    void TearDown() override {
        systemContext_->setMachineStateContext(nullptr);
        machineStateContext_.reset();
        dummyWiFiManager_.reset();
        dummyMqttManager_.reset();
        dummyDisplayManager_.reset();
        dummyHwManager_.reset();
        systemContext_.reset();
        resetConfigDefaults();
    }

    void configureAutomaticBrewWithPreinfusion() {
        Config& config = Config::getInstance();
        config.pidEnabled.set(true);
        config.brewMode.set(Process::BrewMode::AUTOMATIC_BREW);
        config.brewPreInfusionEnabled.set(true);
        config.brewPreInfusionTime.set(3.0);
        config.brewPreInfusionPause.set(2.0);
        config.brewByTimeEnabled.set(true);
        config.brewByTimeTargetTime.set(25.0);
    }

    void advanceStateTime(std::chrono::milliseconds duration) {
        auto pastTime = std::chrono::steady_clock::now() - duration;
        machineStateContext_->updateStateEntryTime(pastTime);
    }

    std::unique_ptr<CleverCoffee::SystemContext>   systemContext_;
    std::unique_ptr<MachineStateContext>           machineStateContext_;
    std::unique_ptr<CleverCoffee::HardwareManager> dummyHwManager_;
    std::unique_ptr<DisplayManager>                dummyDisplayManager_;
    std::unique_ptr<MQTTManager>                   dummyMqttManager_;
    std::unique_ptr<NiceMock<MockWiFiManager>>     dummyWiFiManager_;
};

// =============================================================================
// Flow 1: Complete brew cycle
// PID_NORMAL → BREW_PREINFUSION → BREW_PREINFUSION_PAUSE → BREW_RUNNING →
// BREW_FINISHED → PID_NORMAL
// =============================================================================

TEST_F(StateFlowIntegrationTest, CompletBrewCycle_FullFlow) {
    configureAutomaticBrewWithPreinfusion();

    // --- Step 1: PID_NORMAL with brew start request → transitions to BREW_PREINFUSION ---
    PidNormalState pidNormal;
    machineStateContext_->setBrewStartRequested(true);
    auto transition = pidNormal.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::BREW_PREINFUSION);

    // --- Step 2: BREW_PREINFUSION onEntry → pump enabled, valve open ---
    CleverCoffee::TestHardwareSpy::reset();
    BrewPreinfusionState brewPreinfusion;
    brewPreinfusion.onEntry(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::enablePumpCalls, 1);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::openWaterValveCalls, 1);

    // --- Step 3: Advance past preinfusion time (3s) → transitions to PAUSE ---
    advanceStateTime(std::chrono::milliseconds(3100));
    transition = brewPreinfusion.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::BREW_PREINFUSION_PAUSE);

    // --- Step 4: BREW_PREINFUSION onExit → pump disabled (valve stays open) ---
    CleverCoffee::TestHardwareSpy::reset();
    brewPreinfusion.onExit(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::disablePumpCalls, 1);
    EXPECT_EQ(CleverCoffee::TestHardwareSpy::closeWaterValveCalls, 0);

    // --- Step 5: BREW_PREINFUSION_PAUSE onEntry → pump disabled, valve open ---
    CleverCoffee::TestHardwareSpy::reset();
    BrewPreinfusionPauseState brewPause;
    brewPause.onEntry(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::disablePumpCalls, 1);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::openWaterValveCalls, 1);
    EXPECT_EQ(CleverCoffee::TestHardwareSpy::enablePumpCalls, 0);

    // --- Step 6: Advance past pause time (2s) → transitions to BREW_RUNNING ---
    advanceStateTime(std::chrono::milliseconds(2100));
    transition = brewPause.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::BREW_RUNNING);

    // --- Step 7: BREW_PREINFUSION_PAUSE onExit → pump disabled (valve stays open) ---
    CleverCoffee::TestHardwareSpy::reset();
    brewPause.onExit(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::disablePumpCalls, 1);
    EXPECT_EQ(CleverCoffee::TestHardwareSpy::closeWaterValveCalls, 0);

    // --- Step 8: BREW_RUNNING onEntry → pump enabled, valve open ---
    CleverCoffee::TestHardwareSpy::reset();
    BrewRunningState brewRunning;
    brewRunning.onEntry(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::enablePumpCalls, 1);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::openWaterValveCalls, 1);

    // --- Step 9: Brew stop requested → transitions to BREW_FINISHED ---
    machineStateContext_->setBrewStopRequested(true);
    transition = brewRunning.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::BREW_FINISHED);

    // --- Step 10: BREW_RUNNING onExit → pump disabled, valve CLOSED ---
    CleverCoffee::TestHardwareSpy::reset();
    brewRunning.onExit(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::disablePumpCalls, 1);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::closeWaterValveCalls, 1);

    // --- Step 11: BREW_FINISHED onEntry → no pump/valve changes ---
    CleverCoffee::TestHardwareSpy::reset();
    BrewFinishedState brewFinished;
    brewFinished.onEntry(*machineStateContext_);
    EXPECT_EQ(CleverCoffee::TestHardwareSpy::enablePumpCalls, 0);
    // Valve/pump may not be touched by onEntry

    // --- Step 12: Advance past FINISHED_DISPLAY_TIMEOUT_MS → transitions to PID_NORMAL ---
    advanceStateTime(std::chrono::milliseconds(3100));
    transition = brewFinished.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::PID_NORMAL);
}

// =============================================================================
// Flow 2: Brew abort during preinfusion pause
// PID_NORMAL → BREW_PREINFUSION → BREW_PREINFUSION_PAUSE → (abort) → PID_NORMAL
// =============================================================================

TEST_F(StateFlowIntegrationTest, BrewAbortDuringPreinfusionPause) {
    configureAutomaticBrewWithPreinfusion();

    // Get into BREW_PREINFUSION_PAUSE state
    BrewPreinfusionPauseState brewPause;
    brewPause.onEntry(*machineStateContext_);
    CleverCoffee::TestHardwareSpy::reset();

    // User requests brew stop during pause
    machineStateContext_->setBrewStopRequested(true);
    auto transition = brewPause.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::PID_NORMAL);

    // Verify onExit behavior: pump disabled but valve NOT closed
    // (valve closure for abort from pause relies on valveSafetyShutdownCheck)
    CleverCoffee::TestHardwareSpy::reset();
    brewPause.onExit(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::disablePumpCalls, 1);
    // DOCUMENTING CURRENT BEHAVIOR: BrewPreinfusionPauseState::onExitImpl does NOT close valve.
    // The valve remains open until valveSafetyShutdownCheck() runs externally.
    EXPECT_EQ(CleverCoffee::TestHardwareSpy::closeWaterValveCalls, 0);
}

TEST_F(StateFlowIntegrationTest, BrewAbortDuringPreinfusion) {
    configureAutomaticBrewWithPreinfusion();

    // Enter preinfusion
    BrewPreinfusionState brewPreinfusion;
    brewPreinfusion.onEntry(*machineStateContext_);
    CleverCoffee::TestHardwareSpy::reset();

    // User requests brew stop during preinfusion
    machineStateContext_->setBrewStopRequested(true);
    auto transition = brewPreinfusion.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::PID_NORMAL);

    // onExit: pump disabled, valve NOT closed (same pattern as pause)
    CleverCoffee::TestHardwareSpy::reset();
    brewPreinfusion.onExit(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::disablePumpCalls, 1);
    EXPECT_EQ(CleverCoffee::TestHardwareSpy::closeWaterValveCalls, 0);
}

// =============================================================================
// Flow 3: PID toggle cycle
// PID_NORMAL → PID_DISABLED → PID_NORMAL → brew works (regression test)
// =============================================================================

TEST_F(StateFlowIntegrationTest, PidToggleCycle_BrewWorksAfterReEnable) {
    configureAutomaticBrewWithPreinfusion();

    // --- Step 1: PID_NORMAL → isPidRuntimeEnabled becomes false → PID_DISABLED ---
    PidNormalState pidNormal;
    machineStateContext_->setUserPidEnabled(false);
    auto transition = pidNormal.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::PID_DISABLED);

    // --- Step 2: PID_DISABLED onEntry → pump disabled, valve closed ---
    CleverCoffee::TestHardwareSpy::reset();
    PidDisabledState pidDisabled;
    pidDisabled.onEntry(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::disablePumpCalls, 1);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::closeWaterValveCalls, 1);

    // --- Step 3: Re-enable PID → transitions back to PID_NORMAL ---
    machineStateContext_->setUserPidEnabled(true);
    transition = pidDisabled.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::PID_NORMAL);

    // --- Step 4: PID_NORMAL → brew start → BREW_PREINFUSION (regression: pump must work) ---
    PidNormalState pidNormal2;
    machineStateContext_->setBrewStartRequested(true);
    transition = pidNormal2.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::BREW_PREINFUSION);

    // --- Step 5: Verify pump actually enables on preinfusion entry ---
    CleverCoffee::TestHardwareSpy::reset();
    BrewPreinfusionState brewPreinfusion;
    brewPreinfusion.onEntry(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::enablePumpCalls, 1)
        << "Regression: pump must enable after PID toggle cycle";
    EXPECT_GE(CleverCoffee::TestHardwareSpy::openWaterValveCalls, 1)
        << "Regression: valve must open after PID toggle cycle";
}

// =============================================================================
// Flow 4: Emergency during brew
// BREW_RUNNING → (emergency) → EMERGENCY_STOP (via BaseState::checkTransitions)
// =============================================================================

TEST_F(StateFlowIntegrationTest, EmergencyDuringBrew_HardwareCleanedUp) {
    configureAutomaticBrewWithPreinfusion();

    // Enter brew running state
    BrewRunningState brewRunning;
    brewRunning.onEntry(*machineStateContext_);
    CleverCoffee::TestHardwareSpy::reset();

    // Trigger emergency stop
    machineStateContext_->setEmergencyStop(true);

    // BaseState::checkTransitions detects emergency before checkSpecificTransitions
    auto transition = brewRunning.checkTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::EMERGENCY_STOP);

    // onExit is called by the state machine on transition → hardware cleanup
    CleverCoffee::TestHardwareSpy::reset();
    brewRunning.onExit(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::disablePumpCalls, 1);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::closeWaterValveCalls, 1);
}

TEST_F(StateFlowIntegrationTest, EmergencyDuringPreinfusion_Detected) {
    configureAutomaticBrewWithPreinfusion();

    BrewPreinfusionState brewPreinfusion;
    brewPreinfusion.onEntry(*machineStateContext_);

    machineStateContext_->setEmergencyStop(true);
    auto transition = brewPreinfusion.checkTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::EMERGENCY_STOP);
}

// Regression: after a transient overtemperature trips emergency, recovery must
// restore the runtime PID from config so the machine resumes heating instead of
// getting stuck showing "PID is disabled manually".
// EMERGENCY_STOP → (cleared) → INIT → PID_NORMAL
TEST_F(StateFlowIntegrationTest, EmergencyRecovery_RestoresPidFromConfig) {
    Config::getInstance().pidEnabled.set(true);

    EmergencyStopState emergency;
    emergency.onEntry(*machineStateContext_);
    EXPECT_FALSE(machineStateContext_->isPidRuntimeEnabled());

    // Temperature back to safe range (test sensor reads 0 °C; no ProcessController
    // wired, so EmergencyStopState uses the fallback safe-temp threshold).
    machineStateContext_->setEmergencyStop(false);

    auto transition = emergency.checkTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::INIT);

    // State machine calls onExit on transition → must re-enable runtime PID.
    emergency.onExit(*machineStateContext_);
    EXPECT_TRUE(machineStateContext_->isPidRuntimeEnabled());

    // InitState now routes to PID_NORMAL, not PID_DISABLED.
    InitState init;
    auto      initTransition = init.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(initTransition.has_value());
    EXPECT_EQ(initTransition.value(), MachineStateId::PID_NORMAL);
}

// When PID was disabled in config (user choice), emergency recovery must NOT
// silently re-enable it — config remains the source of truth.
TEST_F(StateFlowIntegrationTest, EmergencyRecovery_RespectsConfigPidDisabled) {
    Config::getInstance().pidEnabled.set(false);

    EmergencyStopState emergency;
    emergency.onEntry(*machineStateContext_);
    machineStateContext_->setEmergencyStop(false);

    emergency.onExit(*machineStateContext_);
    EXPECT_FALSE(machineStateContext_->isPidRuntimeEnabled());
}

// =============================================================================
// Flow 5: Manual flush flow
// PID_NORMAL → MANUAL_FLUSH_RUNNING → PID_NORMAL
// =============================================================================

TEST_F(StateFlowIntegrationTest, ManualFlushFlow_PumpAndValveControlled) {
    Config::getInstance().pidEnabled.set(true);

    // --- Step 1: PID_NORMAL → manual flush start → MANUAL_FLUSH_RUNNING ---
    PidNormalState pidNormal;
    machineStateContext_->setManualFlushStartRequested(true);
    auto transition = pidNormal.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::MANUAL_FLUSH_RUNNING);

    // --- Step 2: ManualFlushRunningState onEntry → pump enabled, valve open ---
    CleverCoffee::TestHardwareSpy::reset();
    ManualFlushRunningState manualFlush;
    manualFlush.onEntry(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::enablePumpCalls, 1);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::openWaterValveCalls, 1);

    // --- Step 3: Manual flush stop → transitions back to PID ---
    machineStateContext_->setManualFlushStopRequested(true);
    transition = manualFlush.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::PID_NORMAL);

    // --- Step 4: onExit → pump disabled, valve closed ---
    CleverCoffee::TestHardwareSpy::reset();
    manualFlush.onExit(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::disablePumpCalls, 1);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::closeWaterValveCalls, 1);
}

TEST_F(StateFlowIntegrationTest, ManualFlush_UpdateKeepsPumpAndValveActive) {
    Config::getInstance().pidEnabled.set(true);

    ManualFlushRunningState manualFlush;
    manualFlush.onEntry(*machineStateContext_);
    CleverCoffee::TestHardwareSpy::reset();

    // Update should maintain pump and valve state
    manualFlush.update(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::enablePumpCalls, 1);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::openWaterValveCalls, 1);
}

// =============================================================================
// Flow 6: Standby and wake
// PID_NORMAL → STANDBY → PID_NORMAL
// =============================================================================

TEST_F(StateFlowIntegrationTest, StandbyAndWake_PidDisabledAndRestored) {
    Config::getInstance().pidEnabled.set(true);

    // --- Step 1: PID_NORMAL → standby requested → STANDBY ---
    PidNormalState pidNormal;
    machineStateContext_->setStandbyRequested(true);
    auto transition = pidNormal.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::STANDBY);

    // --- Step 2: Standby onEntry → pump disabled, valve closed ---
    CleverCoffee::TestHardwareSpy::reset();
    StandbyState standby;
    standby.onEntry(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::disablePumpCalls, 1);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::closeWaterValveCalls, 1);

    // --- Step 3: Normal operation requested → exits standby ---
    machineStateContext_->setNormalOperationRequested(true);
    transition = standby.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::PID_NORMAL);

    // --- Step 4: StandbyState onExit → PID runtime restored ---
    standby.onExit(*machineStateContext_);
    // After exit, the PID state should be based on config (which is true)
    EXPECT_TRUE(machineStateContext_->isPidRuntimeEnabled());
}

TEST_F(StateFlowIntegrationTest, StandbyWakeOnBrewRequest) {
    Config::getInstance().pidEnabled.set(true);

    StandbyState standby;
    standby.onEntry(*machineStateContext_);

    // Brew start request should wake from standby
    machineStateContext_->setBrewStartRequested(true);
    auto transition = standby.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::PID_NORMAL);
}

// =============================================================================
// Additional edge cases
// =============================================================================

TEST_F(StateFlowIntegrationTest, BrewRunningOnExit_AlwaysClosesValve) {
    // This is a critical safety test: BrewRunningState::onExit MUST close the valve
    configureAutomaticBrewWithPreinfusion();

    BrewRunningState brewRunning;
    brewRunning.onEntry(*machineStateContext_);
    CleverCoffee::TestHardwareSpy::reset();

    brewRunning.onExit(*machineStateContext_);

    EXPECT_GE(CleverCoffee::TestHardwareSpy::disablePumpCalls, 1)
        << "Safety: pump must be disabled on brew exit";
    EXPECT_GE(CleverCoffee::TestHardwareSpy::closeWaterValveCalls, 1)
        << "Safety: valve must be closed on brew exit";
}

TEST_F(StateFlowIntegrationTest, BrewFinishedOnExit_AlsoClosesValve) {
    // BrewFinishedState::onExit also cleans up as a safety net
    configureAutomaticBrewWithPreinfusion();

    BrewFinishedState brewFinished;
    brewFinished.onEntry(*machineStateContext_);
    CleverCoffee::TestHardwareSpy::reset();

    brewFinished.onExit(*machineStateContext_);

    EXPECT_GE(CleverCoffee::TestHardwareSpy::disablePumpCalls, 1);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::closeWaterValveCalls, 1);
}

TEST_F(StateFlowIntegrationTest, PidNormalToBrewManualMode_SkipsPreinfusion) {
    Config::getInstance().pidEnabled.set(true);
    Config::getInstance().brewMode.set(Process::BrewMode::MANUAL_BREW);

    PidNormalState pidNormal;
    machineStateContext_->setBrewStartRequested(true);
    auto transition = pidNormal.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    // Manual mode goes directly to BREW_RUNNING
    EXPECT_EQ(transition.value(), MachineStateId::BREW_RUNNING);
}

TEST_F(StateFlowIntegrationTest, PidDisabledDrainsActionRequests) {
    machineStateContext_->setUserPidEnabled(false);

    PidDisabledState pidDisabled;
    pidDisabled.onEntry(*machineStateContext_);

    // Set various requests that should be drained
    machineStateContext_->setBrewStartRequested(true);
    machineStateContext_->setManualFlushStartRequested(true);

    // After update, the flags should be cleared (drained)
    pidDisabled.update(*machineStateContext_);
    EXPECT_FALSE(machineStateContext_->isBrewStartRequested());
    EXPECT_FALSE(machineStateContext_->isManualFlushStartRequested());
}

TEST_F(StateFlowIntegrationTest, BrewPreinfusionToRunning_WhenPreinfusionDisabled) {
    Config::getInstance().pidEnabled.set(true);
    Config::getInstance().brewMode.set(Process::BrewMode::AUTOMATIC_BREW);
    Config::getInstance().brewPreInfusionEnabled.set(false);

    BrewPreinfusionState brewPreinfusion;
    brewPreinfusion.onEntry(*machineStateContext_);

    // With preinfusion disabled, should immediately transition to RUNNING
    auto transition = brewPreinfusion.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::BREW_RUNNING);
}

TEST_F(StateFlowIntegrationTest, BrewPreinfusionToRunning_WhenPauseIsZero) {
    Config::getInstance().pidEnabled.set(true);
    Config::getInstance().brewMode.set(Process::BrewMode::AUTOMATIC_BREW);
    Config::getInstance().brewPreInfusionEnabled.set(true);
    Config::getInstance().brewPreInfusionTime.set(2.0);
    Config::getInstance().brewPreInfusionPause.set(0.0);

    BrewPreinfusionState brewPreinfusion;
    brewPreinfusion.onEntry(*machineStateContext_);

    // Advance past preinfusion time
    advanceStateTime(std::chrono::milliseconds(2100));

    // With pause=0, should skip pause and go directly to RUNNING
    auto transition = brewPreinfusion.checkSpecificTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::BREW_RUNNING);
}

TEST_F(StateFlowIntegrationTest, EmergencyDuringManualFlush_DetectedByBaseState) {
    Config::getInstance().pidEnabled.set(true);

    ManualFlushRunningState manualFlush;
    manualFlush.onEntry(*machineStateContext_);

    machineStateContext_->setEmergencyStop(true);

    // BaseState::checkTransitions handles emergency before specific transitions
    auto transition = manualFlush.checkTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::EMERGENCY_STOP);

    // onExit cleans up hardware
    CleverCoffee::TestHardwareSpy::reset();
    manualFlush.onExit(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::disablePumpCalls, 1);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::closeWaterValveCalls, 1);
}

TEST_F(StateFlowIntegrationTest, PidDisabledDuringBrew_ForcesTransition) {
    configureAutomaticBrewWithPreinfusion();

    BrewRunningState brewRunning;
    brewRunning.onEntry(*machineStateContext_);

    // Disable PID while brew is running
    machineStateContext_->setUserPidEnabled(false);

    // BaseState::checkTransitions forces PID_DISABLED for active operation states
    auto transition = brewRunning.checkTransitions(*machineStateContext_);
    ASSERT_TRUE(transition.has_value());
    EXPECT_EQ(transition.value(), MachineStateId::PID_DISABLED);

    // onExit cleans up
    CleverCoffee::TestHardwareSpy::reset();
    brewRunning.onExit(*machineStateContext_);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::disablePumpCalls, 1);
    EXPECT_GE(CleverCoffee::TestHardwareSpy::closeWaterValveCalls, 1);
}
