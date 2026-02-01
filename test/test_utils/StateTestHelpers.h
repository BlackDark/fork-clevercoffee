/**
 * @file StateTestHelpers.h
 * @brief Helpers for state testing
 * 
 * Provides utilities for testing state machine and state implementations
 */

#pragma once

#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/state/MachineStateContext.h"
#include <memory>

namespace CleverCoffee {
namespace TestUtils {

/**
 * @brief Create a test MachineStateContext with minimal setup
 * @param systemContext SystemContext reference
 * @return Unique pointer to MachineStateContext
 */
std::unique_ptr<MachineStateContext> createTestMachineStateContext(SystemContext& systemContext);

/**
 * @brief Verify state transition occurred
 * @param oldState Previous state ID
 * @param newState New state ID
 * @param actualState Current actual state ID
 * @return true if transition matches expected
 */
bool verifyStateTransition(MachineStateId oldState, MachineStateId newState, MachineStateId actualState);

/**
 * @brief Helper to set up state for testing
 * @param context MachineStateContext to configure
 * @param stateId State to configure for
 */
void setupStateForTesting(MachineStateContext& context, MachineStateId stateId);

} // namespace TestUtils
} // namespace CleverCoffee
