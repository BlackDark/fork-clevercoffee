#pragma once

#include "clevercoffee/coordinators/SensorCoordinator.h"
#include "clevercoffee/coordinators/NetworkCoordinator.h"
#include "clevercoffee/coordinators/UICoordinator.h"

namespace CleverCoffee {

/**
 * @brief Central context for system-wide shared state
 *
 * This class serves as the central hub for all system-wide coordination state,
 * replacing the previous global g_state variable with explicit dependency injection.
 * It manages all coordinators that handle cross-cutting concerns like sensor updates,
 * network operations, and UI refreshes.
 *
 * Design Pattern: Dependency Injection / Service Locator
 * - All components that need access to system state receive a reference to SystemContext
 * - This makes dependencies explicit and improves testability
 * - Thread-safe access is provided through atomic operations in coordinators
 *
 * Lifecycle:
 * 1. Create SystemContext instance during system initialization
 * 2. Pass references to components that need access
 * 3. Call markReady() when all initialization is complete
 * 4. Use isReady() to guard operations that require full initialization
 *
 * Example usage:
 * @code
 * // During initialization
 * SystemContext systemContext;
 *
 * // Inject into components
 * SensorManager sensorMgr(systemContext.sensorCoordinator());
 * NetworkManager netMgr(systemContext.networkCoordinator());
 *
 * // Check initialization state
 * void performOperation(SystemContext& ctx) {
 *     if (!ctx.isReady()) {
 *         return ErrorCode::NOT_INITIALIZED;
 *     }
 *     // ... perform operation ...
 * }
 *
 * // Access coordinators
 * if (ctx.sensorCoordinator().isTemperatureUpdateRunning()) {
 *     // Handle concurrent access
 * }
 * @endcode
 */
class SystemContext {
public:
    SystemContext() = default;

    /**
     * @name Coordinator Access
     * @{
     */

    /**
     * @brief Access sensor coordinator
     *
     * Provides access to the SensorCoordinator for managing sensor operations.
     *
     * @return Reference to the sensor coordinator
     */
    SensorCoordinator& sensorCoordinator() noexcept { return sensorCoordinator_; }

    /**
     * @brief Access sensor coordinator (const overload)
     *
     * @return Const reference to the sensor coordinator
     */
    const SensorCoordinator& sensorCoordinator() const noexcept { return sensorCoordinator_; }

    /**
     * @brief Access network coordinator
     *
     * Provides access to the NetworkCoordinator for managing network state.
     *
     * @return Reference to the network coordinator
     */
    NetworkCoordinator& networkCoordinator() noexcept { return networkCoordinator_; }

    /**
     * @brief Access network coordinator (const overload)
     *
     * @return Const reference to the network coordinator
     */
    const NetworkCoordinator& networkCoordinator() const noexcept { return networkCoordinator_; }

    /**
     * @brief Access UI coordinator
     *
     * Provides access to the UICoordinator for managing display operations.
     *
     * @return Reference to the UI coordinator
     */
    UICoordinator& uiCoordinator() noexcept { return uiCoordinator_; }

    /**
     * @brief Access UI coordinator (const overload)
     *
     * @return Const reference to the UI coordinator
     */
    const UICoordinator& uiCoordinator() const noexcept { return uiCoordinator_; }

    /** @} */

    /**
     * @name System Initialization State
     * @{
     */

    /**
     * @brief Mark the system as fully initialized
     *
     * Should be called once all system components are successfully initialized.
     * Components can check isReady() before performing operations.
     *
     * @post isReady() returns true
     */
    void markReady() noexcept { ready_ = true; }

    /**
     * @brief Check if system is fully initialized
     *
     * @return true if the system has completed initialization, false otherwise
     */
    bool isReady() const noexcept { return ready_; }

    /** @} */

private:
    SensorCoordinator sensorCoordinator_; ///< Manages sensor operation state
    NetworkCoordinator networkCoordinator_; ///< Manages network connection state
    UICoordinator uiCoordinator_;         ///< Manages UI refresh and sleep state
    bool ready_ = false;                  ///< System initialization complete flag
};

} // namespace CleverCoffee
