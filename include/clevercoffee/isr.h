/**
 * @file isr.h
 *
 * @brief Timer - ISR for PID calculation and heater relay output
 *
 */

#pragma once

#include "clevercoffee/constants/Timing.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/hardware/Relay.h"
#include "clevercoffee/types/GlobalTypes.h"

#include <atomic>
#include <cstdint>

// Volatile counters for ISR debugging (track execution without logging)
static volatile bool     isr_enabled         = false;
static volatile uint32_t isr_call_count      = 0;
static volatile uint32_t isr_relay_on_count  = 0;
static volatile uint32_t isr_relay_off_count = 0;

/**
 * @brief ISR-specific SystemContext accessor
 *
 * This is the ONLY place where global SystemContext access is allowed.
 * ISRs cannot take parameters, so we need a static pointer.
 * This is set once during initialization and never changed.
 *
 * @warning Only use this in ISR code. All other code must use dependency injection.
 */

namespace CleverCoffee {
namespace ISR {
// Set the SystemContext for ISR use (called once during initialization)
void setSystemContext(SystemContext* context) noexcept;

// Get the SystemContext for ISR use (only call from ISR code)
SystemContext* getSystemContext() noexcept;
} // namespace ISR
} // namespace CleverCoffee

/**
 * @brief Timer ISR for PID PWM control of heater relay
 *
 * This ISR runs every 10ms and implements PWM (Pulse Width Modulation) control
 * for the heater relay based on PID output. It directly controls the heater relay
 * hardware, bypassing the HardwareManager's state tracking for the heater.
 *
 * IMPORTANT: This ISR directly accesses the heater relay hardware, which means:
 * - The heater relay state may not match HardwareManager::heaterEnabled_ tracking
 * - heaterEnabled_ is atomic to prevent race conditions, but the state is approximate
 * - Only the heater relay is ISR-controlled (for PID PWM); pump/valve relays are main-loop only
 * - This ISR does NOT modify heaterEnabled_ - it only controls the physical relay
 *
 * Safety: All hardware access is null-checked to prevent crashes if hardware fails.
 * The ISR will not execute until the system is fully initialized (isISRReady() returns true).
 */
static inline void IRAM_ATTR onTimer() {
    // Safety check: Get system context (ISR-specific accessor)
    auto* ctx = CleverCoffee::ISR::getSystemContext();
    if (!ctx) {
        return; // System context not available
    }

    // CRITICAL: Check if ISR is ready to execute
    // This prevents ISR from executing before system initialization is complete
    if (!ctx->isISRReady()) {
        return; // System not ready - ISR should not execute
    }

    // Safety check: timer must be initialized and valid before ISR can execute
    // Check for both nullptr AND obviously invalid pointer addresses
    hw_timer_t* timer = ctx->machineTimer();
    if (!timer || (uintptr_t)timer < 0x1000) {
        return;
    }

    isr_enabled = true;
    isr_call_count++;

    // ISR runs every 10ms (10000 microseconds) for PID PWM control
    using CleverCoffee::Timing::ISR_TIMER_INTERVAL_US;
    timerAlarmWrite(timer, ISR_TIMER_INTERVAL_US, true);

    // Read volatile pidOutput once for consistency
    const double       currentPidOutput = ctx->processPidOutput();
    const unsigned int currentCounter   = ctx->isrCounter();

    // PWM control: Turn heater relay on/off based on PID output
    // This directly controls hardware, bypassing HardwareManager state tracking
    if (currentPidOutput <= currentCounter) {
        auto* relay = ctx->hardwareContext().heaterRelay();
        if (relay) {
            relay->off();
            isr_relay_off_count++;
        }
    } else {
        auto* relay = ctx->hardwareContext().heaterRelay();
        if (relay) {
            relay->on();
            isr_relay_on_count++;
        }
    }

    // Increment counter by 10ms per tick
    using CleverCoffee::Timing::ISR_COUNTER_INCREMENT;
    unsigned int newCounter = currentCounter + ISR_COUNTER_INCREMENT;

    // set PID output as relay commands
    if (newCounter >= ctx->processWindowSize()) {
        newCounter = 0;
    }
    ctx->setIsrCounter(newCounter);
}

/**
 * @brief Initialize hardware timers
 */
inline void initTimer1() {
    auto* ctx = CleverCoffee::ISR::getSystemContext();
    if (!ctx) {
        return;
    }
    hw_timer_t* timer = timerBegin(0, 80, true);
    timerAttachInterrupt(timer, &onTimer, true);
    // ISR runs every 10ms (10000 microseconds) for PID PWM control
    using CleverCoffee::Timing::ISR_TIMER_INTERVAL_US;
    timerAlarmWrite(timer, ISR_TIMER_INTERVAL_US, true);
    ctx->setMachineTimer(timer);
}

inline void enableTimer1() {
    auto* ctx = CleverCoffee::ISR::getSystemContext();
    if (!ctx) {
        return;
    }
    hw_timer_t* timer = ctx->machineTimer();
    if (timer) {
        timerAlarmEnable(timer);
    }
}

inline void disableTimer1() {
    auto* ctx = CleverCoffee::ISR::getSystemContext();
    if (!ctx) {
        return;
    }
    hw_timer_t* timer = ctx->machineTimer();
    if (timer) {
        timerAlarmDisable(timer);
    }
}

inline bool isTimer1Enabled() {
    auto* ctx = CleverCoffee::ISR::getSystemContext();
    if (!ctx) {
        return false;
    }
    hw_timer_t* timer = ctx->machineTimer();
    if (!timer) {
        return false;
    }
    return timerAlarmEnabled(timer);
}
