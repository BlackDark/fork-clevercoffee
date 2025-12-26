/**
 * @file isr.h
 *
 * @brief Timer - ISR for PID calculation and heater relay output
 *
 */

#pragma once

#include <atomic>
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/hardware/Relay.h"

// pidOutput moved to g_state.process.pidOutput

void IRAM_ATTR onTimer() {
    timerAlarmWrite(g_state.machine.timer, 10000, true);

    // Read volatile pidOutput once for consistency
    const double currentPidOutput = g_state.process.pidOutput;
    const unsigned int currentCounter = g_state.timing.isrCounter;

    if (currentPidOutput <= currentCounter) {
        g_state.hardware.heaterRelay->off();
    } else {
        g_state.hardware.heaterRelay->on();
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
