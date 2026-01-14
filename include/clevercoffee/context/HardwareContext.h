/**
 * @file HardwareContext.h
 * @brief Hardware component registry and dependency injection container
 */

#pragma once

#include "clevercoffee/hardware/scales/Scale.h"

#include <memory>

// Forward declarations
class U8G2;
class Relay;
class TempSensor;
class Switch;
class GPIOPin;
class LED;

namespace CleverCoffee {

/**
 * @brief Central hardware component registry
 *
 * Manages references to all hardware components in the system.
 * Provides a structured dependency injection container for hardware access.
 *
 * @note Components are owned by HardwareManager and initialized before
 *       being registered here. This class provides read-only access.
 */
class HardwareContext {
  public:
    HardwareContext() = default;

    // ========== Display ==========

    /**
     * @brief Get display instance
     * @return Pointer to U8G2 display (nullptr if not initialized)
     */
    U8G2* display() const noexcept {
        return display_;
    }

    /**
     * @brief Register display instance
     * @param display Pointer to display hardware
     */
    void setDisplay(U8G2* display) noexcept {
        display_ = display;
    }

    // ========== Relays ==========

    /**
     * @brief Get heater relay
     * @return Pointer to heater relay (nullptr if not initialized)
     */
    Relay* heaterRelay() const noexcept {
        return heaterRelay_;
    }

    /**
     * @brief Register heater relay
     * @param relay Pointer to heater relay hardware
     */
    void setHeaterRelay(Relay* relay) noexcept {
        heaterRelay_ = relay;
    }

    /**
     * @brief Get pump relay
     * @return Pointer to pump relay (nullptr if not initialized)
     */
    Relay* pumpRelay() const noexcept {
        return pumpRelay_;
    }

    /**
     * @brief Register pump relay
     * @param relay Pointer to pump relay hardware
     */
    void setPumpRelay(Relay* relay) noexcept {
        pumpRelay_ = relay;
    }

    /**
     * @brief Get valve relay
     * @return Pointer to valve relay (nullptr if not initialized)
     */
    Relay* valveRelay() const noexcept {
        return valveRelay_;
    }

    /**
     * @brief Register valve relay
     * @param relay Pointer to valve relay hardware
     */
    void setValveRelay(Relay* relay) noexcept {
        valveRelay_ = relay;
    }

    // ========== Temperature Sensor ==========

    /**
     * @brief Get temperature sensor
     * @return Pointer to temperature sensor (nullptr if not initialized)
     */
    TempSensor* tempSensor() const noexcept {
        return tempSensor_;
    }

    /**
     * @brief Register temperature sensor
     * @param sensor Pointer to temperature sensor hardware
     */
    void setTempSensor(TempSensor* sensor) noexcept {
        tempSensor_ = sensor;
    }

    // ========== Scale ==========

    /**
     * @brief Get scale instance
     * @return Reference to scale unique_ptr
     */
    const std::unique_ptr<Scale>& scale() const noexcept {
        return scale_;
    }

    /**
     * @brief Get scale raw pointer
     * @return Pointer to scale (nullptr if not initialized)
     */
    Scale* scalePtr() const noexcept {
        return scale_.get();
    }

    /**
     * @brief Register scale instance (transfers ownership)
     * @param scale Unique pointer to scale hardware
     */
    void setScale(std::unique_ptr<Scale> scale) noexcept {
        scale_ = std::move(scale);
    }

    /**
     * @brief Check if scale is bluetooth
     * @return true if bluetooth scale, false otherwise
     */
    bool isBluetoothScale() const noexcept {
        return isBluetoothScale_;
    }

    /**
     * @brief Set bluetooth scale flag
     * @param isBluetooth true if bluetooth scale
     */
    void setIsBluetoothScale(bool isBluetooth) noexcept {
        isBluetoothScale_ = isBluetooth;
    }

    // ========== Switches ==========

    /**
     * @brief Get brew switch
     * @return Pointer to brew switch (nullptr if not initialized)
     */
    Switch* brewSwitch() const noexcept {
        return brewSwitch_;
    }

    /**
     * @brief Register brew switch
     * @param sw Pointer to brew switch hardware
     */
    void setBrewSwitch(Switch* sw) noexcept {
        brewSwitch_ = sw;
    }

    /**
     * @brief Get steam switch
     * @return Pointer to steam switch (nullptr if not initialized)
     */
    Switch* steamSwitch() const noexcept {
        return steamSwitch_;
    }

    /**
     * @brief Register steam switch
     * @param sw Pointer to steam switch hardware
     */
    void setSteamSwitch(Switch* sw) noexcept {
        steamSwitch_ = sw;
    }

    /**
     * @brief Get power switch
     * @return Pointer to power switch (nullptr if not initialized)
     */
    Switch* powerSwitch() const noexcept {
        return powerSwitch_;
    }

    /**
     * @brief Register power switch
     * @param sw Pointer to power switch hardware
     */
    void setPowerSwitch(Switch* sw) noexcept {
        powerSwitch_ = sw;
    }

    /**
     * @brief Get hot water switch
     * @return Pointer to hot water switch (nullptr if not initialized)
     */
    Switch* hotWaterSwitch() const noexcept {
        return hotWaterSwitch_;
    }

    /**
     * @brief Register hot water switch
     * @param sw Pointer to hot water switch hardware
     */
    void setHotWaterSwitch(Switch* sw) noexcept {
        hotWaterSwitch_ = sw;
    }

    /**
     * @brief Get water tank sensor
     * @return Pointer to water tank sensor (nullptr if not initialized)
     */
    Switch* waterTankSensor() const noexcept {
        return waterTankSensor_;
    }

    /**
     * @brief Register water tank sensor
     * @param sensor Pointer to water tank sensor hardware
     */
    void setWaterTankSensor(Switch* sensor) noexcept {
        waterTankSensor_ = sensor;
    }

    // ========== LEDs and GPIO Pins ==========

    /**
     * @brief Get status LED GPIO pin
     * @return Pointer to status LED pin (nullptr if not initialized)
     */
    GPIOPin* statusLedPin() const noexcept {
        return statusLedPin_;
    }

    /**
     * @brief Register status LED GPIO pin
     * @param pin Pointer to GPIO pin hardware
     */
    void setStatusLedPin(GPIOPin* pin) noexcept {
        statusLedPin_ = pin;
    }

    /**
     * @brief Get brew LED GPIO pin
     * @return Pointer to brew LED pin (nullptr if not initialized)
     */
    GPIOPin* brewLedPin() const noexcept {
        return brewLedPin_;
    }

    /**
     * @brief Register brew LED GPIO pin
     * @param pin Pointer to GPIO pin hardware
     */
    void setBrewLedPin(GPIOPin* pin) noexcept {
        brewLedPin_ = pin;
    }

    /**
     * @brief Get steam LED GPIO pin
     * @return Pointer to steam LED pin (nullptr if not initialized)
     */
    GPIOPin* steamLedPin() const noexcept {
        return steamLedPin_;
    }

    /**
     * @brief Register steam LED GPIO pin
     * @param pin Pointer to GPIO pin hardware
     */
    void setSteamLedPin(GPIOPin* pin) noexcept {
        steamLedPin_ = pin;
    }

    /**
     * @brief Get status LED controller
     * @return Pointer to status LED (nullptr if not initialized)
     */
    LED* statusLed() const noexcept {
        return statusLed_;
    }

    /**
     * @brief Register status LED controller
     * @param led Pointer to LED hardware
     */
    void setStatusLed(LED* led) noexcept {
        statusLed_ = led;
    }

    /**
     * @brief Get brew LED controller
     * @return Pointer to brew LED (nullptr if not initialized)
     */
    LED* brewLed() const noexcept {
        return brewLed_;
    }

    /**
     * @brief Register brew LED controller
     * @param led Pointer to LED hardware
     */
    void setBrewLed(LED* led) noexcept {
        brewLed_ = led;
    }

    /**
     * @brief Get steam LED controller
     * @return Pointer to steam LED (nullptr if not initialized)
     */
    LED* steamLed() const noexcept {
        return steamLed_;
    }

    /**
     * @brief Register steam LED controller
     * @param led Pointer to LED hardware
     */
    void setSteamLed(LED* led) noexcept {
        steamLed_ = led;
    }

  private:
    // Display
    U8G2* display_ = nullptr;

    // Relays
    Relay* heaterRelay_ = nullptr;
    Relay* pumpRelay_   = nullptr;
    Relay* valveRelay_  = nullptr;

    // Temperature sensor
    TempSensor* tempSensor_ = nullptr;

    // Scale
    std::unique_ptr<Scale> scale_            = nullptr;
    bool                   isBluetoothScale_ = false;

    // Switches
    Switch* brewSwitch_      = nullptr;
    Switch* steamSwitch_     = nullptr;
    Switch* powerSwitch_     = nullptr;
    Switch* hotWaterSwitch_  = nullptr;
    Switch* waterTankSensor_ = nullptr;

    // LED GPIO pins
    GPIOPin* statusLedPin_ = nullptr;
    GPIOPin* brewLedPin_   = nullptr;
    GPIOPin* steamLedPin_  = nullptr;

    // LED controllers
    LED* statusLed_ = nullptr;
    LED* brewLed_   = nullptr;
    LED* steamLed_  = nullptr;
};

} // namespace CleverCoffee
