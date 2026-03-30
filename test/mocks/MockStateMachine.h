/**
 * @file MockStateMachine.h
 * @brief GMock implementation for StateMachine
 *
 * Provides a mock of the StateMachine public API for testing code that
 * interacts with the state machine (LoopManager, coordinators, etc.)
 * without creating real state objects.
 */

#pragma once

#include <gmock/gmock.h>
#include "clevercoffee/state/MachineStateIds.h"

/**
 * @class MockStateMachine
 * @brief Mock implementation of StateMachine for testing
 *
 * Covers the full public API of StateMachine. Use EXPECT_CALL / ON_CALL
 * to configure behaviour in tests.
 *
 * Example:
 * @code
 * MockStateMachine mockSM;
 * ON_CALL(mockSM, getCurrentStateId()).WillByDefault(Return(MachineStateId::PID_NORMAL));
 * ON_CALL(mockSM, isInitialized()).WillByDefault(Return(true));
 * EXPECT_CALL(mockSM, transitionTo(MachineStateId::STEAM_RUNNING, _)).Times(1);
 * @endcode
 */
class MockStateMachine {
  public:
    MockStateMachine()          = default;
    virtual ~MockStateMachine() = default;

    // ── Lifecycle ───────────────────────────────────────────────────

    MOCK_METHOD(void, initialize, (MachineStateId initialStateId));
    MOCK_METHOD(void, update, ());

    // ── State Transitions ───────────────────────────────────────────

    MOCK_METHOD(void, transitionTo, (MachineStateId newStateId, const char* reason));

    // ── Queries ─────────────────────────────────────────────────────

    MOCK_METHOD(MachineStateId, getCurrentStateId, (), (const, noexcept));
    MOCK_METHOD(const char*, getCurrentStateName, (), (const, noexcept));
    MOCK_METHOD(bool, isInitialized, (), (const, noexcept));
};
