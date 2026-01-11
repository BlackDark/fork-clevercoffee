/**
 * @file isr.h
 *
 * @brief Timer - ISR for PID calculation and heater relay output
 *
 */

#pragma once

#include <atomic>
#include <cstdint>
#include "clevercoffee/types/GlobalTypes.h"
#include "clevercoffee/hardware/Relay.h"

// Volatile counters for ISR debugging (track execution without logging)
static volatile bool isr_enabled = false;
static volatile uint32_t isr_call_count = 0;
static volatile uint32_t isr_relay_on_count = 0;
static volatile uint32_t isr_relay_off_count = 0;

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
void IRAM_ATTR onTimer() {
    // Safety check: Get system context
    auto* ctx = CleverCoffee::getGlobalSystemContext();
    if (!ctx) {
        return;  // System context not available
    }
    
    // CRITICAL: Check if ISR is ready to execute
    // This prevents ISR from executing before system initialization is complete
    if (!ctx->isISRReady()) {
        return;  // System not ready - ISR should not execute
    }
    
    // Safety check: timer must be initialized and valid before ISR can execute
    // Check for both nullptr AND obviously invalid pointer addresses
    hw_timer_t* timer = ctx->machineTimer();
    if (!timer || (uintptr_t)timer < 0x1000) {
        return;
    }
    
    isr_enabled = true;
    isr_call_count++;
    
    timerAlarmWrite(timer, 10000, true);

    // Read volatile pidOutput once for consistency
    const double currentPidOutput = ctx->processPidOutput();
    const unsigned int currentCounter = ctx->isrCounter();

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

    unsigned int newCounter = currentCounter + 10; // += 10 because one tick = 10ms

    // set PID output as relay commands
    if (newCounter >= ctx->processWindowSize()) {
        newCounter = 0;
    }
    ctx->setIsrCounter(newCounter);
}

/**
 * @brief Initialize hardware timers
 */
void initTimer1() {
    auto* ctx = CleverCoffee::getGlobalSystemContext();
    if (!ctx) {
        return;
    }
    hw_timer_t* timer = timerBegin(0, 80, true);
    timerAttachInterrupt(timer, &onTimer, true);
    timerAlarmWrite(timer, 10000, true);
    ctx->setMachineTimer(timer);
}

void enableTimer1() {
    auto* ctx = CleverCoffee::getGlobalSystemContext();
    if (!ctx) {
        return;
    }
    hw_timer_t* timer = ctx->machineTimer();
    if (timer) {
        timerAlarmEnable(timer);
    }
}

void disableTimer1() {
    auto* ctx = CleverCoffee::getGlobalSystemContext();
    if (!ctx) {
        return;
    }
    hw_timer_t* timer = ctx->machineTimer();
    if (timer) {
        timerAlarmDisable(timer);
    }
}

bool isTimer1Enabled() {
    auto* ctx = CleverCoffee::getGlobalSystemContext();
    if (!ctx) {
        return false;
    }
    hw_timer_t* timer = ctx->machineTimer();
    if (!timer) {
        return false;
    }
    return timerAlarmEnabled(timer);
}
