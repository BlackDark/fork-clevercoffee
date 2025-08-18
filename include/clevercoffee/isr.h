/**
 * @file isr.h
 *
 * @brief Timer - ISR for PID calculation and heater relay output
 *
 */

#pragma once

#include "clevercoffee/GlobalState.h"
#include "clevercoffee/hardware/Relay.h"

// pidOutput moved to g_state.process.pidOutput

void IRAM_ATTR onTimer() {
    timerAlarmWrite(g_state.machine.timer, 10000, true);

    if (g_state.process.pidOutput <= g_state.timing.isrCounter) {
        g_state.hardware.heaterRelay->off();
    } else {
        g_state.hardware.heaterRelay->on();
    }

    g_state.timing.isrCounter += 10; // += 10 because one tick = 10ms

    // set PID output as relay commands
    if (g_state.timing.isrCounter >= g_state.process.windowSize) {
        g_state.timing.isrCounter = 0;
    }
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
