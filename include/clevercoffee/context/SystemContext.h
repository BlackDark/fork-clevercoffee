#pragma once

#include "clevercoffee/coordinators/SensorCoordinator.h"

namespace CleverCoffee {

/**
 * @brief Central context for system-wide shared state
 *
 * Replaces global g_state with explicit dependency injection.
 * All coordination flags and system state should be managed here.
 */
class SystemContext {
public:
    SystemContext() = default;

    /**
     * @brief Access sensor coordinator
     */
    SensorCoordinator& sensorCoordinator() noexcept { return sensorCoordinator_; }
    const SensorCoordinator& sensorCoordinator() const noexcept { return sensorCoordinator_; }

    /**
     * @brief Mark the system as fully initialized
     */
    void markReady() noexcept { ready_ = true; }

    /**
     * @brief Check if system is fully initialized
     */
    bool isReady() const noexcept { return ready_; }

private:
    SensorCoordinator sensorCoordinator_;
    bool ready_ = false;
};

} // namespace CleverCoffee
