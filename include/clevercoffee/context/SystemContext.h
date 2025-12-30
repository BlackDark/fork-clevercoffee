#pragma once

#include "clevercoffee/context/HardwareContext.h"
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
class MQTTManager;
class CleverCoffeeWiFiManager;
class WebServerManager;

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

    /**
     * @brief Access hardware context
     *
     * Provides access to the HardwareContext for accessing hardware components.
     *
     * @return Reference to the hardware context
     */
    HardwareContext& hardwareContext() noexcept { return hardwareContext_; }

    /**
     * @brief Access hardware context (const overload)
     *
     * @return Const reference to the hardware context
     */
    const HardwareContext& hardwareContext() const noexcept { return hardwareContext_; }

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

     /**
      * @name Network Managers
      * @{
      */

     /**
      * @brief Register MQTT manager
      * @param manager Pointer to MQTTManager instance (can be nullptr)
      */
     void setMQTTManager(MQTTManager* manager) noexcept { mqttManager_ = manager; }

     /**
      * @brief Get MQTT manager
      * @return Pointer to MQTTManager (may be nullptr if not registered)
      */
     MQTTManager* mqttManager() noexcept { return mqttManager_; }

     /**
      * @brief Get MQTT manager (const)
      * @return Const pointer to MQTTManager (may be nullptr if not registered)
      */
     const MQTTManager* mqttManager() const noexcept { return mqttManager_; }

     /**
      * @brief Register WiFi manager
      * @param manager Pointer to CleverCoffeeWiFiManager instance (can be nullptr)
      */
     void setCleverCoffeeWiFiManager(CleverCoffeeWiFiManager* manager) noexcept { cleverCoffeeWiFiManager_ = manager; }

     /**
      * @brief Get WiFi manager
      * @return Pointer to CleverCoffeeWiFiManager (may be nullptr if not registered)
      */
     CleverCoffeeWiFiManager* cleverCoffeeWiFiManager() noexcept { return cleverCoffeeWiFiManager_; }

     /**
      * @brief Get WiFi manager (const)
      * @return Const pointer to CleverCoffeeWiFiManager (may be nullptr if not registered)
      */
     const CleverCoffeeWiFiManager* cleverCoffeeWiFiManager() const noexcept { return cleverCoffeeWiFiManager_; }

     /**
      * @brief Register WebServer manager
      * @param manager Pointer to WebServerManager instance (can be nullptr)
      */
     void setWebServerManager(WebServerManager* manager) noexcept { webServerManager_ = manager; }

     /**
      * @brief Get WebServer manager
      * @return Pointer to WebServerManager (may be nullptr if not registered)
      */
     WebServerManager* webServerManager() noexcept { return webServerManager_; }

     /**
      * @brief Get WebServer manager (const)
      * @return Const pointer to WebServerManager (may be nullptr if not registered)
      */
     const WebServerManager* webServerManager() const noexcept { return webServerManager_; }

     /** @} */


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
    HardwareContext hardwareContext_;         ///< Hardware component registry
    SensorCoordinator sensorCoordinator_;     ///< Manages sensor operation state
    NetworkCoordinator networkCoordinator_;   ///< Manages network connection state
    UICoordinator uiCoordinator_;             ///< Manages UI refresh and sleep state
    StandbyCoordinator standbyCoordinator_;   ///< Manages standby mode and power management
    bool ready_ = false;                      ///< System initialization complete flag

    // Handler references (non-owning pointers)
    BrewHandler*     brewHandler_     = nullptr;
    HotWaterHandler* hotWaterHandler_ = nullptr;
    PowerHandler*    powerHandler_    = nullptr;
    SteamHandler*    steamHandler_    = nullptr;
    
     // Controller reference (non-owning pointer)
     ProcessController* processController_ = nullptr;

     // Machine state context reference (non-owning pointer)
     MachineStateContext* machineStateContext_ = nullptr;

     // Manager references (non-owning pointers)
     MQTTManager* mqttManager_ = nullptr;
     CleverCoffeeWiFiManager* cleverCoffeeWiFiManager_ = nullptr;
     WebServerManager* webServerManager_ = nullptr;
};

/**
 * @brief Global system context accessor
 * 
 * Returns the global SystemContext instance if initialized.
 * Used by utility functions and inline code that need system context access.
 * 
 * @warning Returns nullptr if context is not initialized
 * @return Pointer to the global SystemContext instance, or nullptr
 */
extern SystemContext* g_systemContext;

/**
 * @brief Set the global system context reference
 * 
 * Called during system initialization to register the SystemContext.
 * 
 * @param context Pointer to the SystemContext instance
 */
inline void setGlobalSystemContext(SystemContext* context) {
    g_systemContext = context;
}

/**
 * @brief Get the global system context reference
 * 
 * @return Pointer to the global SystemContext instance, or nullptr if not initialized
 */
inline SystemContext* getGlobalSystemContext() {
    return g_systemContext;
}

} // namespace CleverCoffee
