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

#include <memory>

/**
 * @class HardwareManager
 * @brief RAII wrapper for hardware components with automatic resource management
 *
 * This class provides safe management of hardware components using RAII principles.
 * It automatically handles component initialization and cleanup, preventing memory leaks
 * and ensuring proper hardware shutdown.
 */
class HardwareManager {
  public:
    /**
     * @brief Constructor - initializes all hardware components
     */
    HardwareManager();

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

    // Relay access methods
    Relay& getHeaterRelay() const {
        return *heaterRelay_;
    }
    Relay& getPumpRelay() const {
        return *pumpRelay_;
    }
    Relay& getValveRelay() const {
        return *valveRelay_;
    }

    // LED access methods
    LED* getStatusLed() const {
        return statusLed_.get();
    }
    LED* getBrewLed() const {
        return brewLed_.get();
    }
    LED* getSteamLed() const {
        return steamLed_.get();
    }

    // Switch access methods
    Switch* getPowerSwitch() const {
        return powerSwitch_.get();
    }
    Switch* getBrewSwitch() const {
        return brewSwitch_.get();
    }
    Switch* getSteamSwitch() const {
        return steamSwitch_.get();
    }
    Switch* getHotWaterSwitch() const {
        return hotWaterSwitch_.get();
    }
    Switch* getWaterTankSensor() const {
        return waterTankSensor_.get();
    }

    // Temperature sensor access
    TempSensor* getTempSensor() const {
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

  private:
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