/**
 * @file HardwareManager.h
 * @brief RAII wrapper for hardware component management
 */

#pragma once

#include "clevercoffee/hardware/GPIOPin.h"
#include "clevercoffee/hardware/IOSwitch.h"
#include "clevercoffee/hardware/LED.h"
#include "clevercoffee/hardware/Relay.h"
#include "clevercoffee/hardware/StandardLED.h"
#include "clevercoffee/hardware/Switch.h"
#include "clevercoffee/hardware/tempsensors/TempSensor.h"
#include "clevercoffee/hardware/tempsensors/TempSensorDallas.h"
#include "clevercoffee/hardware/tempsensors/TempSensorTSIC.h"
#include "clevercoffee/state/IHardwareContext.h"
#include "clevercoffee/state/MachineStateIds.h"

#include <memory>

// Forward declarations
class Config;

namespace CleverCoffee {

/**
 * @class HardwareManager
 * @brief RAII wrapper for hardware components with automatic resource management
 *
 * This class provides safe management of hardware components using RAII principles.
 * It automatically handles component initialization and cleanup, preventing memory leaks
 * and ensuring proper hardware shutdown.
 * 
 * Implements IHardwareContext to provide high-level hardware control interface for states.
 */
class HardwareManager : public IHardwareContext {
  public:
    /**
     * @brief Constructor - initializes all hardware components
     * @param config Configuration reference (not owned, must outlive this instance)
     */
    explicit HardwareManager(const Config& config);

    /**
     * @brief Destructor - automatically cleans up hardware resources
     */
    ~HardwareManager() = default;

    // Disable copy constructor and assignment operator
    HardwareManager(const HardwareManager&)            = delete;
    HardwareManager& operator=(const HardwareManager&) = delete;

    // Enable move constructor and assignment operator
    HardwareManager(HardwareManager&&)            = default;
    HardwareManager& operator=(HardwareManager&&) = default;

    // LED access methods
    LED* getStatusLed() noexcept {
        return statusLed_.get();
    }
    const LED* getStatusLed() const noexcept {
        return statusLed_.get();
    }
    LED* getBrewLed() const noexcept {
        return brewLed_.get();
    }
    LED* getSteamLed() const noexcept {
        return steamLed_.get();
    }

    // Switch access methods
    Switch* getPowerSwitch() const noexcept {
        return powerSwitch_.get();
    }
    Switch* getBrewSwitch() const noexcept {
        return brewSwitch_.get();
    }
    Switch* getSteamSwitch() const noexcept {
        return steamSwitch_.get();
    }
    Switch* getHotWaterSwitch() const noexcept {
        return hotWaterSwitch_.get();
    }
    Switch* getWaterTankSensor() noexcept {
        return waterTankSensor_.get();
    }
    const Switch* getWaterTankSensor() const noexcept {
        return waterTankSensor_.get();
    }

    // Temperature sensor access
    TempSensor* getTempSensor() noexcept {
        return tempSensor_.get();
    }
    const TempSensor* getTempSensor() const noexcept {
        return tempSensor_.get();
    }

    /**
     * @brief Check if all critical hardware is initialized
     * @return true if critical components are ready
     */
    bool isInitialized() const;

    /**
     * @brief Perform safe shutdown of all hardware
     */
    void safeShutdown();

    /**
     * @brief Update LEDs based on machine state
     * @param machineState Current machine state
     * @param temperature Current temperature
     * @param setpoint Target temperature setpoint
     */
    void updateLEDs(MachineStateId machineState, double temperature, double setpoint);

    // === IHardwareContext Implementation ===
    
    // Temperature Sensor
    double getCurrentTemperature() const noexcept override;
    bool hasTemperatureError() const noexcept override;
    
    // Hardware component access (for IHardwareContext)
    Relay* getHeaterRelay() noexcept override;
    Relay* getPumpRelay() noexcept override;
    Relay* getValveRelay() noexcept override;
    bool isWaterTankEmpty() const noexcept override;
    double getWeight() const noexcept override;
    void tareScale() noexcept override;
    void updateHardware() noexcept override;
    
    // Heater Control
    void enableHeater() noexcept override;
    void disableHeater() noexcept override;
    void setHeaterPower(uint8_t percentage) noexcept override;
    
    // Pump Control
    void enablePump() noexcept override;
    void disablePump() noexcept override;
    void setPumpPressure(float bar) noexcept override;
    
    // Valve Control
    void openSteamValve() noexcept override;
    void closeSteamValve() noexcept override;
    void openWaterValve() noexcept override;
    void closeWaterValve() noexcept override;
    
    // Solenoid Control
    void openSolenoid() noexcept override;
    void closeSolenoid() noexcept override;
    
    // Emergency Control
    void emergencyShutdown() noexcept override;

    /**
     * @brief Update safety state from sensors
     * Called periodically to check water tank status
     */
    void updateSafetyState() noexcept;

  private:
    // Configuration reference (not owned)
    const Config& config_;

    // Safety state
    bool emergencyMode_ = false;
    bool waterTankEmpty_ = false;

    // Initialization state tracking for exception safety
    bool relaysInitialized_ = false;
    bool ledsInitialized_ = false;
    bool switchesInitialized_ = false;
    bool tempSensorInitialized_ = false;

    /**
     * @brief Cleanup partial initialization on exception (noexcept)
     *
     * This method is called when initialization fails to ensure hardware
     * is in a safe state. It MUST NOT throw exceptions.
     * Cleanup happens in REVERSE order of initialization.
     *
     * SAFETY CRITICAL: Relays (especially heater) MUST be turned off.
     */
    void cleanupPartialInit() noexcept;
    // GPIO Pins for relays (stack allocated)
    GPIOPin heaterRelayPin_;
    GPIOPin pumpRelayPin_;
    GPIOPin valveRelayPin_;

    // Relays (managed with smart pointers)
    std::unique_ptr<Relay> heaterRelay_;
    std::unique_ptr<Relay> pumpRelay_;
    std::unique_ptr<Relay> valveRelay_;

    // GPIO Pins for LEDs (managed with smart pointers)
    std::unique_ptr<GPIOPin> statusLedPin_;
    std::unique_ptr<GPIOPin> brewLedPin_;
    std::unique_ptr<GPIOPin> steamLedPin_;

    // LEDs (managed with smart pointers)
    std::unique_ptr<LED> statusLed_;
    std::unique_ptr<LED> brewLed_;
    std::unique_ptr<LED> steamLed_;

    // Switches (managed with smart pointers)
    std::unique_ptr<Switch> powerSwitch_;
    std::unique_ptr<Switch> brewSwitch_;
    std::unique_ptr<Switch> steamSwitch_;
    std::unique_ptr<Switch> hotWaterSwitch_;
    std::unique_ptr<Switch> waterTankSensor_;

    // Temperature sensor (managed with smart pointer)
    std::unique_ptr<TempSensor> tempSensor_;

    /**
     * @brief Initialize relay components
     */
    void initializeRelays();

    /**
     * @brief Initialize LED components
     */
    void initializeLEDs();

    /**
     * @brief Initialize switch components
     */
    void initializeSwitches();

    /**
     * @brief Initialize temperature sensor
     */
    void initializeTemperatureSensor();
};

}  // namespace CleverCoffee