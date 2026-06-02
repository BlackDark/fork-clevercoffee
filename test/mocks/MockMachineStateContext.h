/**
 * @file MockMachineStateContext.h
 * @brief GMock implementation of MachineStateContext for testing state transitions
 */

#pragma once

#include "clevercoffee/state/MachineStateIds.h"
#include <chrono>
#include <gmock/gmock.h>

// Forward declarations
class DisplayManager;
class HardwareManager;
class SensorManager;
class CleverCoffeeWiFiManager;
class MQTTManager;
class U8G2;
class TempSensor;
class Switch;
class Relay;
class LED;
class Scale;

namespace CleverCoffee {
class SystemContext;
}

/**
 * @class MockMachineStateContext
 * @brief GMock-based mock of MachineStateContext for unit testing state logic
 *
 * This mock allows state tests to control all context inputs and verify
 * that states make correct transition decisions based on sensor readings,
 * emergency conditions, and process states.
 */
class MockMachineStateContext {
  public:
    MockMachineStateContext() = default;
    virtual ~MockMachineStateContext() = default;

    // === Hardware Access ===
    MOCK_METHOD(CleverCoffee::SystemContext&, systemContext, (), (const));
    MOCK_METHOD(DisplayManager*, getDisplayManager, (), (const));
    MOCK_METHOD(HardwareManager*, getHardwareManager, (), (const));
    MOCK_METHOD(SensorManager*, getSensorManager, (), (const));
    MOCK_METHOD(CleverCoffeeWiFiManager*, getWiFiManager, (), (const));
    MOCK_METHOD(MQTTManager*, getMQTTManager, (), (const));

    // === Hardware Component Access ===
    MOCK_METHOD(TempSensor*, getTemperatureSensor, (), (const));
    MOCK_METHOD(Switch*, getWaterTankSensor, (), (const));
    MOCK_METHOD(Switch*, getBrewSwitch, (), (const));
    MOCK_METHOD(Switch*, getSteamSwitch, (), (const));
    MOCK_METHOD(Switch*, getHotWaterSwitch, (), (const));
    MOCK_METHOD(Switch*, getPowerSwitch, (), (const));
    MOCK_METHOD(Relay*, getHeaterRelay, (), (const));
    MOCK_METHOD(Relay*, getPumpRelay, (), (const));
    MOCK_METHOD(Relay*, getValveRelay, (), (const));
    MOCK_METHOD(LED*, getStatusLED, (), (const));
    MOCK_METHOD(LED*, getBrewLED, (), (const));
    MOCK_METHOD(LED*, getSteamLED, (), (const));
    MOCK_METHOD(Scale*, getScale, (), (const));

    // === Sensor Data Access ===
    MOCK_METHOD(double, getCurrentTemperature, (), (const));
    MOCK_METHOD(bool, hasTemperatureError, (), (const));
    MOCK_METHOD(bool, isWaterTankFull, (), (const));
    MOCK_METHOD(float, getCurrentPressure, (), (const));
    MOCK_METHOD(float, getFilteredPressure, (), (const));
    MOCK_METHOD(float, getCurrentWeight, (), (const));
    MOCK_METHOD(float, getCurrentBrewWeight, (), (const));
    MOCK_METHOD(bool, hasScaleError, (), (const));
    MOCK_METHOD(bool, hasSensorError, (), (const));

    // === Process Control Functions ===
    MOCK_METHOD(bool, isBrewActive, (), (const));
    MOCK_METHOD(bool, isManualFlushActive, (), (const));
    MOCK_METHOD(bool, isSteamActive, (), (const));
    MOCK_METHOD(bool, isHotWaterActive, (), (const));
    MOCK_METHOD(bool, isBackflushActive, (), (const));

    // === System State Access ===
    MOCK_METHOD(bool, isPidRuntimeEnabled, (), (const));
    MOCK_METHOD(bool, isPidConfigEnabled, (), (const));
    MOCK_METHOD(bool, isEmergencyStop, (), (const));
    MOCK_METHOD(bool, shouldEnterStandby, (), (const));
    MOCK_METHOD(unsigned long, getStandbyRemainingTime, (), (const));

    // === Timing Functions ===
    MOCK_METHOD(unsigned long, getCurrentTime, (), (const));
    MOCK_METHOD(void, resetStandbyTimer, (MachineStateId), (const));

    // === Control Functions ===
    MOCK_METHOD(void, setSteamMode, (bool), (const));
    MOCK_METHOD(void, setPidRuntimeState, (bool), (const));
    MOCK_METHOD(void, setManualFlushState, (bool), (const));
    MOCK_METHOD(void, setHotWaterState, (bool), (const));
    MOCK_METHOD(void, setSteamState, (bool), (const));
    MOCK_METHOD(void, disableWaterOperations, (), (const));
    MOCK_METHOD(void, enableWaterOperations, (), (const));
    MOCK_METHOD(void, enterSafeMode, (), (const));
    MOCK_METHOD(void, exitSafeMode, (), (const));
    MOCK_METHOD(void, enterStandbyMode, (), (const));
    MOCK_METHOD(void, exitStandbyMode, (), (const));

    // === User Activity ===
    MOCK_METHOD(bool, hasUserActivity, (), (const));
    MOCK_METHOD(bool, shouldExitStandby, (), (const));
    MOCK_METHOD(void, performSafeShutdown, (), (const));

    // === Display Functions ===
    MOCK_METHOD(U8G2*, getDisplay, (), (const));
    MOCK_METHOD(void, setDisplayPowerSave, (int), (const));

    // === Logging Functions ===
    MOCK_METHOD(void, logStateTransition, (MachineStateId, MachineStateId, const char*), (const));
    MOCK_METHOD(void, logStateEntry, (MachineStateId, const char*), (const));
    MOCK_METHOD(void, logStateExit, (MachineStateId, const char*), (const));

    // === MQTT Integration ===
    MOCK_METHOD(void, resetMqttReconnectCount, (), (const));

    // === Configuration Access ===
    MOCK_METHOD(unsigned long, getBackflushFillTimeMs, (), (const));
    MOCK_METHOD(unsigned long, getBackflushFlushTimeMs, (), (const));

    // === State Timing Functions ===
    MOCK_METHOD(unsigned long, getStateElapsedTimeMs, (), (const));
    MOCK_METHOD(bool, hasStateTimeoutElapsed, (unsigned long), (const));
    MOCK_METHOD(void, updateStateEntryTime, (std::chrono::steady_clock::time_point));
};
