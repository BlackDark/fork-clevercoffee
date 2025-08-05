/**
 * @file WebSocketEvents.cpp
 * @brief WebSocket event functions implementation - external definitions to avoid library conflicts
 */

#include "WebSocketEvents.h"
#include "Logger.h"

// External function declarations - implemented in embeddedWebserver.h
// These avoid including the header that causes HTTP method conflicts
extern void sendTempEvent(const double currentTemp, const double targetTemp, const double heaterPower);
extern void sendWeightEvent();
extern void serverSetup();

// Wrapper functions that match our interface
void sendTempEvent(double temp, double setpoint, double pidOutput) {
    // Call the external function with matching parameters
    ::sendTempEvent(temp, setpoint, pidOutput);
}

void sendWeightEvent() {
    // Call the external function
    ::sendWeightEvent();
}

void serverSetup() {
    // Call the external function
    ::serverSetup();
}