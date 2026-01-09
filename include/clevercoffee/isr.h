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

void IRAM_ATTR onTimer() {
    // Safety check: timer must be initialized and valid before ISR can execute
    // Check for both nullptr AND obviously invalid pointer addresses
    auto* ctx = CleverCoffee::getGlobalSystemContext();
    hw_timer_t* timer = ctx ? ctx->machineTimer() : nullptr;
    if (!timer || (uintptr_t)timer < 0x1000) {
        return;
    }
    
    // SafetyCheck: SystemContext must be initialized before we can use it
    if (!ctx) {
        return;
    }
    
    isr_enabled = true;
    isr_call_count++;
    
    timerAlarmWrite(timer, 10000, true);

    // Read volatile pidOutput once for consistency
    const double currentPidOutput = ctx->processPidOutput();
    const unsigned int currentCounter = ctx->isrCounter();

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
