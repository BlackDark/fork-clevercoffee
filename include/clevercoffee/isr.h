/**
 * @file isr.h
 *
 * @brief Timer - ISR for PID calculation and heater relay output
 *
 */

#pragma once

#include <atomic>
#include <cstdint>
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/hardware/Relay.h"

// pidOutput moved to g_state.process.pidOutput

void IRAM_ATTR onTimer() {
    // Safety check: timer must be initialized and valid before ISR can execute
    // Check for both nullptr AND obviously invalid pointer addresses
    if (!g_state.machine.timer || (uintptr_t)g_state.machine.timer < 0x1000) {
        return;
    }
    
    // SafetyCheck: SystemContext must be initialized before we can use it
    auto* ctx = CleverCoffee::getGlobalSystemContext();
    if (!ctx) {
        return;
    }
    
    timerAlarmWrite(g_state.machine.timer, 10000, true);

    // Read volatile pidOutput once for consistency
    const double currentPidOutput = g_state.process.pidOutput;
    const unsigned int currentCounter = g_state.timing.isrCounter;

    if (currentPidOutput <= currentCounter) {
        auto* relay = ctx->hardwareContext().heaterRelay();
        if (relay) {
            relay->off();
        }
    } else {
        auto* relay = ctx->hardwareContext().heaterRelay();
        if (relay) {
            relay->on();
        }
    }

    unsigned int newCounter = currentCounter + 10; // += 10 because one tick = 10ms

    // set PID output as relay commands
    if (newCounter >= g_state.process.windowSize) {
        newCounter = 0;
    }
    g_state.timing.isrCounter = newCounter;
}

/**
 * @brief Initialize hardware timers
 */
void initTimer1() {
    g_state.machine.timer = timerBegin(0, 80, true);
    timerAttachInterrupt(g_state.machine.timer, &onTimer, true);
    timerAlarmWrite(g_state.machine.timer, 10000, true);
}

void enableTimer1() {
    timerAlarmEnable(g_state.machine.timer);
}

void disableTimer1() {
    timerAlarmDisable(g_state.machine.timer);
}

bool isTimer1Enabled() {
    return timerAlarmEnabled(g_state.machine.timer);
}
