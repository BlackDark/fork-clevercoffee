/**
 * @file ValveState.h
 * @brief Valve state enumeration for shared hardware
 */

#pragma once

namespace CleverCoffee::Hardware {

/**
 * @enum ValveState
 * @brief State of the shared valve relay hardware
 *
 * Steam and water valves share the same physical relay. This enum tracks
 * which valve(s) should be open, ensuring correct relay control.
 */
enum class ValveState {
    CLOSED,     ///< Both valves closed - relay should be OFF
    STEAM_OPEN, ///< Only steam valve open - relay should be ON
    WATER_OPEN, ///< Only water valve open - relay should be ON
    BOTH_OPEN   ///< Both valves open - relay should be ON (if hardware supports)
};

} // namespace CleverCoffee::Hardware
