/**
 * @file Relay.h
 *
 * @brief A relay connected to a GPIO pin
 */
#pragma once

#include "clevercoffee/defaults.h"
#include "clevercoffee/hardware/GPIOPin.h"

#include <cstdint>

/**
 * @file Relay.h Relay control class
 * @brief This class provides control for relay switches
 *
 * ISR SAFETY: The on() and off() methods are ISR-safe.
 * - They only perform single GPIO writes (atomic on ESP32)
 * - They do not call any blocking functions
 * - They do not allocate memory
 * - They can be safely called from ISR context (e.g., PID PWM timer ISR)
 *
 * Note: The ISR calls these methods directly for heater relay PWM control.
 * The HardwareManager::heaterEnabled_ state tracking is NOT updated by ISR calls,
 * so that state is approximate when ISR is active.
 */
class Relay {
  public:
    /**
     * @brief Constructor
     *
     * @param gpioInstance GPIO pin this relay is connected to
     * @param trigger Trigger type this relay requires
     */
    explicit Relay(GPIOPin&                   gpioInstance,
                   Hardware::RelayTriggerType trigger = Hardware::RelayTriggerType::HIGH_TRIGGER);

    /**
     * @brief Switch relay on
     * @note ISR-safe: Can be called from interrupt context
     */
    void on() const noexcept;

    /**
     * @brief Switch relay off
     * @note ISR-safe: Can be called from interrupt context
     */
    void off() const noexcept;

    /**
     * @brief Get the GPIO pin this relay is connected to
     * @return GPIO pin of the relay
     */
    [[nodiscard]] GPIOPin& getGPIOInstance() const noexcept;

    /**
     * @brief Get the trigger type of this relay
     * @return Trigger type
     */
    [[nodiscard]] constexpr Hardware::RelayTriggerType getTriggerType() const noexcept {
        return relayTrigger;
    }

  private:
    GPIOPin&                   gpio;
    Hardware::RelayTriggerType relayTrigger;
};
