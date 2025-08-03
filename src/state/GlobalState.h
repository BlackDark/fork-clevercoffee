/**
 * @file GlobalState.h
 * @brief Consolidated global state management
 *
 * This replaces scattered global variables with organized data structures.
 * Migration strategy: Start with one GlobalState, then gradually reduce to only needed data.
 */

#pragma once

#include "../Config.h"
#include "../defaults.h"
#include <memory>
#include <Arduino.h>

// Forward declarations
class U8G2;
class Relay;
class TempSensor;
class MQTTManager;
class Timer;
class PID;
class Config;

/**
 * @brief Process control related state
 */
struct ProcessState {
    double temperature = 0.0;
    double setpoint = 95.0;  // Default brew setpoint
    double pidOutput = 0.0;
    bool steamMode = false;
    bool pidEnabled = true;
    double currBrewTime = 0.0;
    double steamSetpointValue = 120.0;  // Will be initialized from config
    bool brewPidDisabled = false;
    // useBDPID, usePonM, brewPidDelay moved to config access only
    double previousInput = 0.0;

    // PID - values for offline brew detection
    double aggbKi = 0.0;
    double aggbKd = 0.0;
    double aggKi = 0.0;
    double aggKd = 0.0;
};

/**
 * @brief System coordination flags for update management
 */
struct CoordinationState {
    bool temperatureUpdateRunning = false;
    bool websiteUpdateRunning = false;
    bool hassioUpdateRunning = false;
    bool displayUpdateRunning = false;
    bool displayBufferReady = false;
    bool mqttUpdateRunning = false;
};

/**
 * @brief Hardware component references
 */
struct HardwareRefs {
    U8G2* display = nullptr;
    Relay* heaterRelay = nullptr;
    Relay* pumpRelay = nullptr;
    Relay* valveRelay = nullptr;
    TempSensor* tempSensor = nullptr;

    // Switches and sensors
    void* brewSwitch = nullptr;
    void* steamSwitch = nullptr;
    void* powerSwitch = nullptr;
    void* hotWaterSwitch = nullptr;
    void* waterTankSensor = nullptr;

    // LEDs
    std::unique_ptr<Relay>* statusLed = nullptr;
    std::unique_ptr<Relay>* brewLed = nullptr;
    std::unique_ptr<Relay>* steamLed = nullptr;
};

/**
 * @brief Network and communication state
 */
struct NetworkState {
    bool offlineMode = false;
    unsigned int wifiReconnects = 0;
    unsigned long lastWifiConnectionAttempt = 0;
    // hostname, mqtt_enabled, mqtt_hassio_enabled moved to config access only
    bool hassioFailed = false;
    bool mqtt_was_connected = false;
    unsigned long lastTempEvent = 0;
    unsigned long tempEventInterval = 1000;
    std::unique_ptr<MQTTManager>* mqttManager = nullptr;
};

/**
 * @brief Timing and debug related state
 */
struct TimingState {
    unsigned long previousMillistemp = 0;
    unsigned long windowStartTime = 0;
    unsigned long previousMillisMQTT = 0;
    const unsigned long intervalPressure = 100;
    unsigned long previousMillisPressure = 0;
    Timer* printDisplayTimer = nullptr;
};

/**
 * @brief Standby and power management
 */
struct StandbyState {
    // standbyModeOn, standbyModeTime moved to config access only
    unsigned long standbyModeRemainingTimeMillis = 0;
    unsigned long standbyModeStartTimeMillis = 0;
};

/**
 * @brief Scale and pressure sensor data
 */
struct SensorState {
    float inputPressure = 0.0;
    float inputPressureFilter = 0.0;
    bool scaleTareOn = false;
    bool scaleCalibrationOn = false;
    double currBrewWeight = 0.0;
    double currReadingWeight = 0.0;
};

/**
 * @brief Machine state and brewing
 */
struct MachineStateData {
    int machineState = 0;  // LegacyMachineState
    int lastmachinestate = -1;
    int lastmachinestatepid = -1;
    bool emergencyStop = false;
    bool steamON = false;
    bool backflushOn = false;
    bool waterTankFull = true;
    bool systemInitialized = false;
};

/**
 * @brief Display and UI state
 */
struct DisplayState {
    int displayOffline = 0;
};

/**
 * @brief Central global state container
 *
 * This struct contains all global state organized into logical groups.
 * Migration approach:
 * 1. Replace all extern declarations with access to this single struct
 * 2. Pass this struct (or parts of it) to managers via dependency injection
 * 3. Gradually move data ownership into individual managers
 * 4. Reduce this struct to only truly shared state
 */
struct GlobalState {
    ProcessState process;
    CoordinationState coordination;
    HardwareRefs hardware;
    NetworkState network;
    TimingState timing;
    StandbyState standby;
    SensorState sensors;
    MachineStateData machine;
    DisplayState display;

    // System-wide references (initialized later)
    Config* config = nullptr;
    PID* pid = nullptr;

    // System version info
    const char* sysVersion = VERSION;
    String systemVersion = String(VERSION);
};

// Single global state instance - replaces all scattered globals
extern GlobalState g_state;
