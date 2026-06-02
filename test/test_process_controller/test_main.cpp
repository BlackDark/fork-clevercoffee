/**
 * @file test_main.cpp
 * @brief Integration tests for ProcessController
 *
 * Tests safety-critical behavior:
 * - Emergency stop on overtemperature
 * - PID output clamping to safe bounds
 * - Setpoint management (brew/steam switching)
 * - Initialization from config
 */

#include <gtest/gtest.h>
#include <gmock/gmock.h>
#include "../test_support.h"
#include "../mocks/MockHardwareManager.h"
#include "../mocks/MockDisplayManager.h"
#include "../mocks/MockMQTTManager.h"

// Include implementations (PlatformIO native tests don't link src/)
#include "../mocks/ConfigStubs.cpp"
#include "../../src/Logger.cpp"
#include "../../src/control/EmergencyStopManager.cpp"
#include "../../src/control/ProcessController.cpp"
#include "../../src/context/SystemContext.cpp"
#include "../../src/coordinators/SensorCoordinator.cpp"
#include "../../src/coordinators/MaintenanceCoordinator.cpp"

// Stub implementations for dependencies we don't actually use in these tests
// Stub for MachineStateContext methods that are not inline
void MachineStateContext::setHotWaterActivity(bool) noexcept {}
void MachineStateContext::setBrewStartRequested(bool) noexcept {}
void MachineStateContext::setSteamStartRequested(bool) noexcept {}
void MachineStateContext::setNormalOperationRequested(bool) noexcept {}
void MachineStateContext::setBackflushEnterRequested(bool) noexcept {}
void MachineStateContext::setBackflushCycleStartRequested(bool) noexcept {}
void MachineStateContext::setBackflushStopRequested(bool) noexcept {}
bool MachineStateContext::applyBackflushMode(bool active) noexcept {
    (void)active;
    return true;
}

// Stub for MachineStateContext emergency stop tracking
// Note: The inline setEmergencyStop() in header modifies emergencyStop_ member,
// but we need to track state for tests since we can't instantiate MachineStateContext.
// Using thread_local to allow parallel test execution.
namespace {
    thread_local bool emergencyStopState = false;
}

bool MachineStateContext::isEmergencyStop() const {
    return emergencyStopState;
}

// Helper for tests to set emergency stop state
void setTestEmergencyStop(bool active) {
    emergencyStopState = active;
}

// Stub for Relay
void Relay::off() const noexcept {}
void Relay::on() const noexcept {}

using namespace CleverCoffee;
using ::testing::Return;
using ::testing::NiceMock;
using ::testing::_;

// ============================================================================
// TEST FIXTURE
// ============================================================================

class ProcessControllerIntegrationTest : public ::testing::Test {
protected:
    std::unique_ptr<SystemContext> systemContext_;
    std::unique_ptr<ProcessController> controller_;

    NiceMock<MockHardwareManager> mockHardwareManager_;
    NiceMock<MockDisplayManager> mockDisplayManager_;
    NiceMock<MockMQTTManager> mockMqttManager_;

    void SetUp() override {
        // Create real SystemContext
        systemContext_ = std::make_unique<SystemContext>();

        // Create a PID controller (required for computePID tests)
        // Note: PID needs input, output, and setpoint pointers
        static double pidInput = 0, pidOutput = 0, pidSetpoint = 95.0;
        static PID pid(&pidInput, &pidOutput, &pidSetpoint, 50.0, 0.5, 15.0, P_ON_E, DIRECT);
        systemContext_->setPidController(&pid);

        systemContext_->markReady();

        // Configure safe default temperature
        ON_CALL(mockHardwareManager_, getCurrentTemperature())
            .WillByDefault(Return(25.0));
        ON_CALL(mockHardwareManager_, hasTemperatureError())
            .WillByDefault(Return(false));

        // Configure default config values
        Config& config = Config::getInstance();
        config.brewSetpoint.set(95.0);  // Match DEFAULT_BREW_SETPOINT_C
        config.steamSetpoint.set(130.0);
        config.pidRegularKp.set(50.0);
        config.pidRegularTn.set(100.0);
        config.pidRegularTv.set(15.0);
        config.emergencyStopTemp.set(145.0);

        // Create real ProcessController
        controller_ = std::make_unique<ProcessController>(
            config,
            *systemContext_,
            mockHardwareManager_,
            mockDisplayManager_,
            mockMqttManager_
        );
    }

    void TearDown() override {
        controller_.reset();
        systemContext_.reset();
        emergencyStopState = false;  // Reset for next test
    }
};

// ============================================================================
// INITIALIZATION TESTS
// ============================================================================

/**
 * TEST: ProcessController initializes successfully
 */
TEST_F(ProcessControllerIntegrationTest, InitializesSuccessfully) {
    bool result = controller_->initialize();
    EXPECT_TRUE(result) << "ProcessController should initialize successfully";
}

/**
 * TEST: ProcessController loads setpoint from config
 */
TEST_F(ProcessControllerIntegrationTest, LoadsSetpointFromConfig) {
    controller_->initialize();

    double setpoint = controller_->getSetpoint();
    EXPECT_GT(setpoint, 0.0) << "Setpoint should be loaded from config";
    EXPECT_DOUBLE_EQ(95.0, setpoint) << "Setpoint should match config value";
}

// ============================================================================
// SAFETY-CRITICAL TESTS
// ============================================================================

/**
 * TEST: Emergency stop triggers on overtemperature
 *
 * CRITICAL SAFETY TEST: Verifies the machine shuts down when temperature
 * exceeds the emergency threshold (default 145°C)
 */
TEST_F(ProcessControllerIntegrationTest, EmergencyStopOnOvertemperature) {
    controller_->initialize();

    Config& config = Config::getInstance();
    const double emergencyThreshold = config.emergencyStopTemp.get();
    const double dangerousTemp = emergencyThreshold + 5.0;  // 150°C

    // Simulate dangerous temperature - need 3 consecutive readings for debounce
    // Note: We set temperature_ directly since SensorCoordinator has no sensor in tests
    controller_->temperature_ = dangerousTemp;

    // First two calls build up debounce counter
    controller_->testEmergencyConditions();
    controller_->testEmergencyConditions();

    // Third call should trigger emergency
    bool emergencyTriggered = controller_->testEmergencyConditions();

    EXPECT_TRUE(emergencyTriggered)
        << "Emergency should trigger after 3 consecutive overtemperature readings";

    // Verify emergency actions were taken
    EXPECT_FALSE(controller_->isPIDEnabled())
        << "PID should be disabled after emergency";
    EXPECT_EQ(0.0, controller_->getPIDOutput())
        << "PID output should be zeroed after emergency";
}

/**
 * TEST: PID output is clamped to safe bounds
 *
 * SAFETY TEST: Verifies PID output stays within 0-1000 range
 * regardless of input conditions
 */
TEST_F(ProcessControllerIntegrationTest, PIDOutputClampedToSafeBounds) {
    controller_->initialize();
    controller_->setPIDEnabled(true);

    // Set a large temperature error to push PID to extremes
    ON_CALL(mockHardwareManager_, getCurrentTemperature())
        .WillByDefault(Return(20.0));  // Far below setpoint

    // Run multiple update cycles to let PID respond
    for (int i = 0; i < 5; i++) {
        controller_->updateTemperature();
        controller_->computePID();
    }

    double output = controller_->getPIDOutput();
    EXPECT_GE(output, 0.0) << "PID output should not be negative";
    EXPECT_LE(output, 1000.0) << "PID output should not exceed 1000";
}

/**
 * TEST: PID output clamped when temperature above setpoint
 */
TEST_F(ProcessControllerIntegrationTest, PIDOutputClampedWhenHot) {
    controller_->initialize();
    controller_->setPIDEnabled(true);

    // Temperature well above setpoint
    ON_CALL(mockHardwareManager_, getCurrentTemperature())
        .WillByDefault(Return(100.0));  // Above 95°C setpoint

    // Run multiple update cycles
    for (int i = 0; i < 5; i++) {
        controller_->updateTemperature();
        controller_->computePID();
    }

    double output = controller_->getPIDOutput();
    EXPECT_GE(output, 0.0) << "PID output should not be negative";
    EXPECT_LE(output, 1000.0) << "PID output should not exceed 1000";
}

// ============================================================================
// SETPOINT MANAGEMENT TESTS
// ============================================================================

/**
 * TEST: Setpoint switches to steam temperature when steam active
 */
TEST_F(ProcessControllerIntegrationTest, SwitchesToSteamSetpoint) {
    controller_->initialize();

    double brewSetpoint = controller_->getSetpoint();
    EXPECT_DOUBLE_EQ(95.0, brewSetpoint);

    // Activate steam mode
    controller_->updateSetpoint(true);  // steamActive = true

    double steamSetpoint = controller_->getSetpoint();
    EXPECT_GT(steamSetpoint, brewSetpoint)
        << "Steam setpoint should be higher than brew setpoint";
    EXPECT_DOUBLE_EQ(130.0, steamSetpoint);
}

/**
 * TEST: Setpoint returns to brew temperature when steam deactivated
 */
TEST_F(ProcessControllerIntegrationTest, ReturnsToBrewSetpoint) {
    controller_->initialize();

    // Switch to steam
    controller_->updateSetpoint(true);
    EXPECT_DOUBLE_EQ(130.0, controller_->getSetpoint());

    // Switch back to brew
    controller_->updateSetpoint(false);
    EXPECT_DOUBLE_EQ(95.0, controller_->getSetpoint())
        << "Should return to brew setpoint when steam deactivated";
}

// ============================================================================
// PID STATE MANAGEMENT TESTS
// ============================================================================

/**
 * TEST: PID can be enabled and disabled
 */
TEST_F(ProcessControllerIntegrationTest, PIDCanBeEnabledAndDisabled) {
    controller_->initialize();

    controller_->setPIDEnabled(true);
    EXPECT_TRUE(controller_->isPIDEnabled());

    controller_->setPIDEnabled(false);
    EXPECT_FALSE(controller_->isPIDEnabled());
}

/**
 * TEST: PID should be enabled for normal brewing state
 */
TEST_F(ProcessControllerIntegrationTest, PIDEnabledForNormalState) {
    controller_->initialize();

    bool shouldEnable = controller_->shouldPIDBeEnabled(MachineStateId::PID_NORMAL);
    EXPECT_TRUE(shouldEnable) << "PID should be enabled in PID_NORMAL state";
}

/**
 * TEST: PID should be disabled for disabled state
 */
TEST_F(ProcessControllerIntegrationTest, PIDDisabledForDisabledState) {
    controller_->initialize();

    bool shouldEnable = controller_->shouldPIDBeEnabled(MachineStateId::PID_DISABLED);
    EXPECT_FALSE(shouldEnable) << "PID should be disabled in PID_DISABLED state";
}

// ============================================================================
// BUG FIX: Safe shutdown must NOT enter emergency mode (Bug #2)
// After PowerHandler calls performSafeShutdown, hardware must remain usable.
// Root cause was: performSafeShutdown() called emergencyShutdown() which set
// emergencyMode_=true permanently, blocking all subsequent hardware ops.
// ============================================================================

/**
 * TEST: performSafeShutdown calls safeHardwareShutdown (not emergencyShutdown)
 *
 * Verifies the bug fix: normal power-off must NOT set emergencyMode_.
 * If emergencyShutdown() were called, pump/valve would be permanently blocked.
 */
TEST_F(ProcessControllerIntegrationTest, SafeShutdownDoesNotCallEmergencyShutdown) {
    controller_->initialize();

    EXPECT_CALL(mockHardwareManager_, emergencyShutdown()).Times(0);
    EXPECT_CALL(mockHardwareManager_, safeHardwareShutdown()).Times(1);

    controller_->performSafeShutdown();
}

/**
 * TEST: clearEmergencyMode is NOT called during safe shutdown
 *
 * clearEmergencyMode() is reserved for clearing real emergencies.
 * performSafeShutdown() must not call it — the machine was never in emergency mode.
 */
TEST_F(ProcessControllerIntegrationTest, ClearEmergencyModeNotCalledDuringSafeShutdown) {
    controller_->initialize();

    EXPECT_CALL(mockHardwareManager_, clearEmergencyMode()).Times(0);
    EXPECT_CALL(mockHardwareManager_, safeHardwareShutdown()).Times(1);

    controller_->performSafeShutdown();
}

// Note: main() is provided by test/main.cpp
