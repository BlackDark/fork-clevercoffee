/**
 * @file WebSocketEvents.h
 * @brief WebSocket event functions isolated from main webserver to avoid library conflicts
 */

#pragma once

/**
 * @brief Send temperature event via WebSocket
 * @param temp Current temperature
 * @param setpoint Target temperature setpoint
 * @param pidOutput PID controller output
 */
void sendTempEvent(double temp, double setpoint, double pidOutput);

/**
 * @brief Send weight event via WebSocket
 */
void sendWeightEvent();

/**
 * @brief Initialize webserver and WebSocket functionality
 */
void serverSetup();