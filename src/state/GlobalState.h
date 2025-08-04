/**
 * @file GlobalState.h
 * @brief Consolidated global state management
 *
 * This replaces scattered global variables with organized data structures.
 * Migration strategy: Start with one GlobalState, then gradually reduce to only needed data.
 */

#pragma once

#include "../defaults.h"
#include "../utils/Timer.h"
#include "./brewStates.h"
#include <Arduino.h>
#include <memory>
#include <map>
#include <functional>
#include <cstring>

// standby.h
#define TIME_TO_DISPLAY_OFF        10
#define TIME_TO_DISPLAY_OFF_MILLIS (TIME_TO_DISPLAY_OFF * 60 * 1000)

// Include MachineState enum constants from main.cpp
enum LegacyMachineState {
    kInit = 0,
    kPidNormal = 20,
    kBrew = 30,
    kManualFlush = 35,
    kHotWater = 40,
    kSteam = 50,
    kBackflush = 60,
    kWaterTankEmpty = 70,
    kEmergencyStop = 80,
    kPidDisabled = 90,
    kStandby = 95,
    kSensorError = 100,
    kEepromError = 110
};

struct cmp_str {
    bool operator()(char const *a, char const *b) const {
        return std::strcmp(a, b) < 0;
    }
};

#include "../hardware/scales/Scale.h"

// Forward declarations
class U8G2;
class Relay;
class TempSensor;
class MQTTManager;
class PID;
class Config;
class Switch;

/**
 * @brief Process control related state
 */
struct ProcessState {
        double temperature = 0.0;
        double setpoint = 95.0; // Default brew setpoint
        double pidOutput = 0.0;
        bool pidEnabled = true;

        double currBrewTime = 0.0;
        long startingTime = 0; // Start time of brew
        double totalTargetBrewTime = 0.0; // Target brew time in seconds

        double steamSetpointValue = 120.0; // Will be initialized from config
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
        Scale* scale = nullptr;

        // Switches and sensors
        Switch* brewSwitch = nullptr;
        Switch* steamSwitch = nullptr;
        Switch* powerSwitch = nullptr;
        Switch* hotWaterSwitch = nullptr;
        Switch* waterTankSensor = nullptr;

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
        std::map<const char*, const char*, cmp_str> mqttVars;
        std::map<const char*, std::function<double()>, cmp_str> mqttSensors;
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
        std::unique_ptr<Timer> loopWaterTank = nullptr;
        std::unique_ptr<Timer> hassioDiscoveryTimer = nullptr;
        std::unique_ptr<Timer> printDisplayTimer = nullptr;
        Timer* loopWaterTank2 = nullptr;
        Timer* hassioDiscoveryTimer2 = nullptr;
        Timer* printDisplayTimer2 = nullptr;
};

/**
 * @brief Standby and power management
 */
struct StandbyState {
        // standbyModeOn, standbyModeTime moved to config access only
        unsigned long standbyModeRemainingTimeMillis = 0;
        unsigned long standbyModeStartTimeMillis = 0;

        unsigned long standbyModeRemainingTimeDisplayOffMillis = TIME_TO_DISPLAY_OFF_MILLIS;
        unsigned long lastStandbyTimeMillis = 0;
        unsigned long timeSinceStandbyMillis = 0;
        //unsigned long standbyModeStartTimeMillis = millis();
        //unsigned long standbyModeRemainingTimeMillis = static_cast<long>(Config::getInstance().get<double>("standby.time")) * 60 * 1000;
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
        bool scaleFailure = false;

        // Pressure filter variables
        float inX = 0.0f;
        float inY = 0.0f;
        float inOld = 0.0f;
        float inSum = 0.0f;

        // steamHandler
        uint8_t currStateSteamSwitch;

        // powerHandler
        bool currStatePowerSwitchPressed = false;
        bool lastPowerSwitchPressed = false;
        unsigned long systemInitializedTime = 0;
        unsigned long firstSwitchPressTime = 0;
        bool trackingPressTime = false;

        // brewHandler
        BrewSwitchState currBrewSwitchState = kBrewSwitchIdle;
        BrewState currBrewState = kBrewIdle;
        ManualFlushState currManualFlushState = kManualFlushIdle;
        BackflushState currBackflushState = kBackflushIdle;

        uint8_t brewSwitchReading = LOW;
        uint8_t currReadingBrewSwitch = LOW;
        bool brewSwitchWasOff = false;
};

/**
 * @brief Machine state and brewing
 */
struct MachineStateData {
        LegacyMachineState machineState = LegacyMachineState::kInit; // LegacyMachineState
        LegacyMachineState lastmachinestate = LegacyMachineState::kInit;
        int lastmachinestatepid = -1;
        bool emergencyStop = false;
        bool steamON = false;
        bool steamFirstON = false;
        bool backflushOn = false;
        int currBackflushCycles = 1;
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
 * @brief Legacy state for debugging purposes.
 */
struct DebugState {
    String hotWaterStateDebug = "off";
    String lastHotWaterStateDebug = "off";
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
        DebugState debug;

        // System-wide references (initialized later)
        Config* config = nullptr;
        PID* pid = nullptr;

        // System version info
        const char* sysVersion = VERSION;
        String systemVersion = String(VERSION);
};

// Single global state instance - replaces all scattered globals
extern GlobalState g_state;
