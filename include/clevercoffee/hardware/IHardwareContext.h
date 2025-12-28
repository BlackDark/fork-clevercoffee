/**
 * @file IHardwareContext.h
 * @brief High-level hardware control interface for states
 */

#pragma once

#include <cstdint>

namespace CleverCoffee {

/**
 * @class IHardwareContext
 * @brief Abstract interface for hardware control
 * 
 * States use this interface to control hardware. The interface expresses
 * intent ("enable pump") not mechanism ("turn on relay").
 * 
 * HardwareManager implements this interface and handles:
 * - Safety checks (e.g., don't enable pump if tank empty)
 * - Logging
 * - Hardware abstraction (relay details hidden)
 */
class IHardwareContext {
public:
    virtual ~IHardwareContext() = default;
    
    // === Heater Control ===
    
    /**
     * @brief Enable the heating element
     */
    virtual void enableHeater() noexcept = 0;
    
    /**
     * @brief Disable the heating element
     */
    virtual void disableHeater() noexcept = 0;
    
    /**
     * @brief Set heater power level
     * @param percentage Power level 0-100%
     */
    virtual void setHeaterPower(uint8_t percentage) noexcept = 0;
    
    // === Pump Control ===
    
    /**
     * @brief Enable the pump
     * Safety check: Will not enable if water tank is empty
     */
    virtual void enablePump() noexcept = 0;
    
    /**
     * @brief Disable the pump
     */
    virtual void disablePump() noexcept = 0;
    
    /**
     * @brief Set pump pressure
     * @param bar Pressure in bar (0.0 - 9.0 typical)
     */
    virtual void setPumpPressure(float bar) noexcept = 0;
    
    // === Valve Control ===
    
    /**
     * @brief Open the steam valve
     */
    virtual void openSteamValve() noexcept = 0;
    
    /**
     * @brief Close the steam valve
     */
    virtual void closeSteamValve() noexcept = 0;
    
    /**
     * @brief Open the water valve
     */
    virtual void openWaterValve() noexcept = 0;
    
    /**
     * @brief Close the water valve
     */
    virtual void closeWaterValve() noexcept = 0;
    
    // === Solenoid Control ===
    
    /**
     * @brief Open the solenoid
     */
    virtual void openSolenoid() noexcept = 0;
    
    /**
     * @brief Close the solenoid
     */
    virtual void closeSolenoid() noexcept = 0;
    
    // === Emergency Control ===
    
    /**
     * @brief Emergency shutdown - disable all hardware immediately
     * 
     * Called when:
     * - Emergency stop button pressed
     * - Temperature exceeds safety limit
     * - Critical sensor failure
     */
    virtual void emergencyShutdown() noexcept = 0;
};

}  // namespace CleverCoffee
