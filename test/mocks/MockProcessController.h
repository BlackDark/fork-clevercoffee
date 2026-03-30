/**
 * @file MockProcessController.h
 * @brief GMock implementation for ProcessController
 *
 * Provides a mock of the ProcessController public API for testing code
 * that depends on process control (PID, temperature, brewing) without
 * requiring real hardware or PID library initialisation.
 */

#pragma once

#include <gmock/gmock.h>
#include "clevercoffee/state/MachineStateIds.h"

/**
 * @class MockProcessController
 * @brief Mock implementation of ProcessController for testing
 *
 * Covers the full public API of ProcessController. Use EXPECT_CALL / ON_CALL
 * to set up expectations and return values in your tests.
 *
 * Example:
 * @code
 * MockProcessController mockPC;
 * ON_CALL(mockPC, getCurrentTemperature()).WillByDefault(Return(93.5));
 * EXPECT_CALL(mockPC, computePID()).Times(1);
 * @endcode
 */
class MockProcessController {
  public:
    MockProcessController()          = default;
    virtual ~MockProcessController() = default;

    // ── Lifecycle ───────────────────────────────────────────────────

    MOCK_METHOD(bool, initialize, ());
    MOCK_METHOD(void, update, ());

    // ── Process Control ─────────────────────────────────────────────

    MOCK_METHOD(void, updateProcessControl, (MachineStateId machineState));
    MOCK_METHOD(void, updateTemperature, ());
    MOCK_METHOD(void, computePID, ());
    MOCK_METHOD(void, updatePIDState, (MachineStateId machineState));

    // ── PID Tuning ──────────────────────────────────────────────────

    MOCK_METHOD(void, setPIDTunings, (bool usePonM));
    MOCK_METHOD(void, setBrewDetectionPIDTunings, ());
    MOCK_METHOD(void, setSteamPIDTunings, ());
    MOCK_METHOD(void, updateSetpoint, (bool steamActive));

    // ── Queries ─────────────────────────────────────────────────────

    MOCK_METHOD(bool, shouldPIDBeEnabled, (MachineStateId machineState), (const));
    MOCK_METHOD(double, getCurrentTemperature, (), (const));
    MOCK_METHOD(double, getPIDOutput, (), (const));
    MOCK_METHOD(double, getSetpoint, (), (const));
    MOCK_METHOD(bool, isPIDEnabled, (), (const));
    MOCK_METHOD(double, getAggKp, (), (const));
    MOCK_METHOD(double, getAggKi, (), (const));
    MOCK_METHOD(double, getAggKd, (), (const));
    MOCK_METHOD(int, getWindowSize, (), (const));
    MOCK_METHOD(double, getAggbKi, (), (const));
    MOCK_METHOD(double, getAggbKd, (), (const));

    // ── Mutators ────────────────────────────────────────────────────

    MOCK_METHOD(void, setPIDEnabled, (bool enabled));
    MOCK_METHOD(double, getCurrBrewTime, (), (const));
    MOCK_METHOD(void, setCurrBrewTime, (double brewTime));
    MOCK_METHOD(double, getTotalTargetBrewTime, (), (const));
    MOCK_METHOD(void, setTotalTargetBrewTime, (double brewTime));
    MOCK_METHOD(bool, isBrewPidDisabled, (), (const));
    MOCK_METHOD(void, setBrewPidDisabled, (bool disabled));

    // ── Safety ──────────────────────────────────────────────────────

    MOCK_METHOD(void, emergencyStop, ());
    MOCK_METHOD(void, performSafeShutdown, ());
    MOCK_METHOD(bool, testEmergencyConditions, ());
    MOCK_METHOD(bool, isEmergencyCleared, (double temperature), (const));
};
