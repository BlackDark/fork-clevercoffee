#pragma once

#include "clevercoffee/coordinators/SensorCoordinator.h"
#include "clevercoffee/coordinators/NetworkCoordinator.h"
#include "clevercoffee/coordinators/UICoordinator.h"
#include "clevercoffee/coordinators/StandbyCoordinator.h"

// Forward declarations for handlers
class BrewHandler;
class HotWaterHandler;
class PowerHandler;
class SteamHandler;
class ProcessController;
class MachineStateContext;

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

    /**
     * @brief Access standby coordinator
     *
     * Provides access to the StandbyCoordinator for managing standby mode.
     *
     * @return Reference to the standby coordinator
     */
    StandbyCoordinator& standbyCoordinator() noexcept { return standbyCoordinator_; }

    /**
     * @brief Access standby coordinator (const overload)
     *
     * @return Const reference to the standby coordinator
     */
    const StandbyCoordinator& standbyCoordinator() const noexcept { return standbyCoordinator_; }

    /** @} */

    /**
     * @name Handler Registration and Access
     * @{
     */

    /**
     * @brief Register brew handler
     * @param handler Pointer to BrewHandler instance (can be nullptr)
     */
    void setBrewHandler(BrewHandler* handler) noexcept { brewHandler_ = handler; }

    /**
     * @brief Get brew handler
     * @return Pointer to BrewHandler (may be nullptr if not registered)
     */
    BrewHandler* brewHandler() noexcept { return brewHandler_; }

    /**
     * @brief Get brew handler (const)
     * @return Const pointer to BrewHandler (may be nullptr if not registered)
     */
    const BrewHandler* brewHandler() const noexcept { return brewHandler_; }

    /**
     * @brief Register hot water handler
     * @param handler Pointer to HotWaterHandler instance (can be nullptr)
     */
    void setHotWaterHandler(HotWaterHandler* handler) noexcept { hotWaterHandler_ = handler; }

    /**
     * @brief Get hot water handler
     * @return Pointer to HotWaterHandler (may be nullptr if not registered)
     */
    HotWaterHandler* hotWaterHandler() noexcept { return hotWaterHandler_; }

    /**
     * @brief Get hot water handler (const)
     * @return Const pointer to HotWaterHandler (may be nullptr if not registered)
     */
    const HotWaterHandler* hotWaterHandler() const noexcept { return hotWaterHandler_; }

    /**
     * @brief Register power handler
     * @param handler Pointer to PowerHandler instance (can be nullptr)
     */
    void setPowerHandler(PowerHandler* handler) noexcept { powerHandler_ = handler; }

    /**
     * @brief Get power handler
     * @return Pointer to PowerHandler (may be nullptr if not registered)
     */
    PowerHandler* powerHandler() noexcept { return powerHandler_; }

    /**
     * @brief Get power handler (const)
     * @return Const pointer to PowerHandler (may be nullptr if not registered)
     */
    const PowerHandler* powerHandler() const noexcept { return powerHandler_; }

    /**
     * @brief Register steam handler
     * @param handler Pointer to SteamHandler instance (can be nullptr)
     */
    void setSteamHandler(SteamHandler* handler) noexcept { steamHandler_ = handler; }

    /**
     * @brief Get steam handler
     * @return Pointer to SteamHandler (may be nullptr if not registered)
     */
    SteamHandler* steamHandler() noexcept { return steamHandler_; }

    /**
     * @brief Get steam handler (const)
     * @return Const pointer to SteamHandler (may be nullptr if not registered)
     */
    const SteamHandler* steamHandler() const noexcept { return steamHandler_; }

    /**
     * @brief Register process controller
     * @param controller Pointer to ProcessController instance (can be nullptr)
     */
    void setProcessController(ProcessController* controller) noexcept { processController_ = controller; }

    /**
     * @brief Get process controller
     * @return Pointer to ProcessController (may be nullptr if not registered)
     */
    ProcessController* processController() noexcept { return processController_; }

     /**
      * @brief Get process controller (const)
      * @return Const pointer to ProcessController (may be nullptr if not registered)
      */
     const ProcessController* processController() const noexcept { return processController_; }

     /**
      * @brief Register machine state context
      * @param context Pointer to MachineStateContext instance (can be nullptr)
      */
     void setMachineStateContext(MachineStateContext* context) noexcept { machineStateContext_ = context; }

     /**
      * @brief Get machine state context
      * @return Pointer to MachineStateContext (may be nullptr if not registered)
      */
     MachineStateContext* machineStateContext() noexcept { return machineStateContext_; }

     /**
      * @brief Get machine state context (const)
      * @return Const pointer to MachineStateContext (may be nullptr if not registered)
      */
     const MachineStateContext* machineStateContext() const noexcept { return machineStateContext_; }

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
    StandbyCoordinator standbyCoordinator_; ///< Manages standby mode and power management
    bool ready_ = false;                  ///< System initialization complete flag

    // Handler references (non-owning pointers)
    BrewHandler*     brewHandler_     = nullptr;
    HotWaterHandler* hotWaterHandler_ = nullptr;
    PowerHandler*    powerHandler_    = nullptr;
    SteamHandler*    steamHandler_    = nullptr;
    
     // Controller reference (non-owning pointer)
     ProcessController* processController_ = nullptr;

     // Machine state context reference (non-owning pointer)
     MachineStateContext* machineStateContext_ = nullptr;
};

} // namespace CleverCoffee
