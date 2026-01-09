/**
 * @file GlobalState.h
 * @brief Global state type definitions (DEPRECATED)
 *
 * @deprecated This file is deprecated and will be removed in Phase 5.
 *             Use SystemContext and coordinators instead.
 *             All new code should use dependency injection.
 *
 * This file now only contains type definitions needed by SystemContext.
 * The actual global state variable (g_state) has been completely eliminated.
 * State is now managed entirely through SystemContext private members.
 * See SystemContext for state management details.
 */

#pragma once

// DEPRECATED: GlobalState struct and associated types only.
// The global g_state variable has been completely removed.
// State is now encapsulated in SystemContext as private members.
// See SystemContext for the new state management pattern.
//
#if defined(__GNUC__)
#define DEPRECATED __attribute__((deprecated))
#elif defined(_MSC_VER)
#define DEPRECATED __declspec(deprecated)
#else
#define DEPRECATED
#endif

#include "clevercoffee/defaults.h"
#include "clevercoffee/state/MachineStateIds.h"
#include "clevercoffee/utils/ModernTimer.h"

#include <Arduino.h>
#include <cstring>
#include <functional>
#include <map>
#include <memory>

// standby.h
constexpr int TIME_TO_DISPLAY_OFF = 10;
constexpr unsigned long TIME_TO_DISPLAY_OFF_MILLIS = TIME_TO_DISPLAY_OFF * 60 * 1000;

struct cmp_str {
    bool operator()(char const* a, char const* b) const {
        return std::strcmp(a, b) < 0;
    }
};

#include "clevercoffee/hardware/scales/Scale.h"

// Forward declarations
class U8G2;
class Relay;
class TempSensor;
class MQTTManager;
class ProcessController;
class CleverCoffeeWiFiManager;
class WebServerManager;
class PID;
class Config;

// Forward declaration for SystemContext
namespace CleverCoffee {
class SystemContext;
}
class Switch;
class LED;
class GPIOPin;
class BrewHandler;
class HotWaterHandler;
class PowerHandler;
class SteamHandler;

extern const char*      WIFI_PASSWORD;
constexpr unsigned long wifiConnectionDelay   = WIFICONNECTIONDELAY;
constexpr unsigned int  maxWifiReconnects     = MAXWIFIRECONNECTS;
constexpr double        EmergencyStopTemp     = 145;
constexpr int           waterTankCountsNeeded = 3; // Number of same readings to change water tank sensing

// Scale connection constants
constexpr unsigned long SCALE_CONNECTION_CHECK_INTERVAL = 500;   // Check every 500 milliseconds
constexpr unsigned long SCALE_CONNECTION_TIMEOUT        = 5000;  // 5 seconds timeout
constexpr unsigned long SCALE_RECONNECTION_TIMEOUT      = 30000; // 30 seconds before giving up

namespace GlobalStateNamespace {

/**
 * @brief Process control related state
 */
struct ProcessState {
    double temperature = 0.0;
    double setpoint    = 95.0; // Default brew setpoint
    double pidOutput   = 0.0;
    bool   pidEnabled  = true;

    double currBrewTime        = 0.0;
    long   startingTime        = 0;    // Start time of brew
    double totalTargetBrewTime = 0.0;  // Target brew time in seconds

    double steamSetpointValue = 120.0; // Will be initialized from config
    bool   brewPidDisabled    = false;
    // useBDPID, usePonM, brewPidDelay moved to config access only
    double previousInput = 0.0;

    // PID - values for offline brew detection
    double aggbKi = 0.0;
    double aggbKd = 0.0;
    double aggKi  = 0.0;
    double aggKd  = 0.0;

    int windowSize = 1000;
};

/**
 * @brief System coordination flags for update management
 */
struct CoordinationState {
    bool temperatureUpdateRunning = false;
    bool websiteUpdateRunning     = false;
    bool hassioUpdateRunning      = false;
    bool displayUpdateRunning     = false;
    bool displayBufferReady       = false;
    // bool mqttUpdateRunning = false;
    bool               setupDone         = false;
    ProcessController* processController = nullptr;
};

/**
 * @brief Handler instances for organized control
 */
struct HandlerRefs {
    BrewHandler*     brewHandler     = nullptr;
    HotWaterHandler* hotWaterHandler = nullptr;
    PowerHandler*    powerHandler    = nullptr;
    SteamHandler*    steamHandler    = nullptr;
};

/**
 * @brief Hardware component references
 */
struct HardwareRefs {
    U8G2*                  display     = nullptr;
    Relay*                 heaterRelay = nullptr;
    Relay*                 pumpRelay   = nullptr;
    Relay*                 valveRelay  = nullptr;
    TempSensor*            tempSensor  = nullptr;
    std::unique_ptr<Scale> scale       = nullptr;

    bool isBluetoothScale = false;

    // Switches and sensors
    Switch* brewSwitch      = nullptr;
    Switch* steamSwitch     = nullptr;
    Switch* powerSwitch     = nullptr;
    Switch* hotWaterSwitch  = nullptr;
    Switch* waterTankSensor = nullptr;

    // LEDs
    GPIOPin* statusLedPin = nullptr;
    GPIOPin* brewLedPin   = nullptr;
    GPIOPin* steamLedPin  = nullptr;
    LED*     statusLed    = nullptr;
    LED*     brewLed      = nullptr;
    LED*     steamLed     = nullptr;
};

/**
 * @brief Network and communication state
 */
struct NetworkState {
    CleverCoffeeWiFiManager* cleverCoffeeWiFiManager = nullptr;
    WebServerManager*        webServerManager        = nullptr;

    bool          offlineMode               = false;
    unsigned int  wifiReconnects            = 0;
    unsigned long lastWifiConnectionAttempt = 0;
    // hostname, mqtt_enabled, mqtt_hassio_enabled moved to config access only
    unsigned long lastTempEvent     = 0;
    unsigned long tempEventInterval = 1000;

    MQTTManager*                                            mqttManager = nullptr;
    std::map<const char*, const char*, cmp_str>             mqttVars;
    std::map<const char*, std::function<double()>, cmp_str> mqttSensors;
    bool                                                    mqtt_was_connected        = false;
    unsigned int                                            MQTTReCnctCount           = 0;
    unsigned long                                           lastMQTTConnectionAttempt = 0;
    bool                                                    hassioFailed              = false;
};

/**
 * @brief Timing and debug related state
 */
struct TimingState {
    unsigned long                     previousMillistemp     = 0;
    unsigned long                     previousMillisMQTT     = 0;
    static constexpr unsigned long    intervalPressure       = 100;
    unsigned long                     previousMillisPressure = 0;
    std::unique_ptr<MillisecondTimer> loopWaterTank          = nullptr;
    std::unique_ptr<MillisecondTimer> hassioDiscoveryTimer   = nullptr;
    std::unique_ptr<MillisecondTimer> printDisplayTimer      = nullptr;
    MillisecondTimer*                 loopWaterTank2         = nullptr;
    MillisecondTimer*                 hassioDiscoveryTimer2  = nullptr;
    MillisecondTimer*                 printDisplayTimer2     = nullptr;

    // isr + windowSize
    unsigned int  isrCounter      = 0;
    unsigned long windowStartTime = 0;
};

/**
 * @brief Standby and power management
 */
struct StandbyState {
    // standbyModeOn, standbyModeTime moved to config access only
    unsigned long standbyModeRemainingTimeMillis = 0;
    unsigned long standbyModeStartTimeMillis     = 0;

    unsigned long standbyModeRemainingTimeDisplayOffMillis = TIME_TO_DISPLAY_OFF_MILLIS;
    unsigned long lastStandbyTimeMillis                    = 0;
    unsigned long timeSinceStandbyMillis                   = 0;
    // unsigned long standbyModeStartTimeMillis = millis();
    // unsigned long standbyModeRemainingTimeMillis = static_cast<long>(Config::getInstance().standbyTime.get()) * 60 *
    // 1000;
};

/**
 * @brief Scale and pressure sensor data
 */
struct SensorState {
    float  inputPressure       = 0.0;
    float  inputPressureFilter = 0.0;
    double currBrewWeight      = 0.0;
    double currReadingWeight   = 0.0;

    // scale
    bool                   scaleFailure       = false;
    bool                   scaleTareOn        = false;
    bool                   scaleCalibrationOn = false;
    int                    shottimerCounter   = 10;
    float                  preBrewWeight      = 0;   // weight before brew started
    static constexpr float scaleDelayValue    = 2.5; // delay compensation in grams
    bool                   autoTareInProgress = false;
    unsigned long          autoTareStartTime  = 0;

    // bluetooth scale
    unsigned long lastScaleConnectionCheck   = 0;
    unsigned long scaleConnectionFailureTime = 0;
    bool          scaleConnectionLost        = false;
    float         lastValidWeight            = 0;
    bool          brewByWeightFallbackActive = false;

    // scale error handling and retry logic
    int           scaleReadErrorCount         = 0;     // Consecutive read errors
    int           scaleMaxRetries             = 5;     // Maximum retries before marking as failed
    unsigned long lastScaleErrorTime          = 0;     // Time of last error
    unsigned long scaleErrorCooldownMs        = 1000;  // Cooldown between retries
    bool          scaleInErrorRecovery        = false; // In error recovery mode

    // Pressure filter variables
    float inX   = 0.0f;
    float inY   = 0.0f;
    float inOld = 0.0f;
    float inSum = 0.0f;

    // Handler state - kept for backward compatibility during transition
    // TODO: Move these into handler classes gradually
    uint8_t currStateSteamSwitch;

    bool          currStatePowerSwitchPressed = false;
    bool          lastPowerSwitchPressed      = false;
    unsigned long systemInitializedTime       = 0;
    unsigned long firstSwitchPressTime        = 0;
    bool          trackingPressTime           = false;

    SwitchState currBrewSwitchState = SwitchState::IDLE;

    uint8_t brewSwitchReading     = LOW;
    uint8_t currReadingBrewSwitch = LOW;
    bool    brewSwitchWasOff      = false;

    SwitchState   currHotWaterSwitchState   = SwitchState::IDLE;
    uint8_t       hotWaterSwitchReading     = LOW;
    uint8_t       currReadingHotWaterSwitch = LOW;
    double        currPumpOnTime            = 0.0;
    unsigned long pumpStartingTime          = 0;

    // water
    int waterTankCheckConsecutiveReads = 0; // Counter for consecutive readings of water tank sensor
};

struct MachineStateFlags {
    bool requestBrewStart        = false;
    bool requestBrewStop         = false;
    bool requestHotWaterStart    = false;
    bool requestHotWaterStop     = false;
    bool requestManualFlushStart = false;
    bool requestManualFlushStop  = false;
    bool requestBackflushStart   = false;
    bool requestBackflushStop    = false;
    bool requestSteamStart       = false;
    bool requestSteamStop        = false;
    bool requestShutdown         = false;
    bool requestStandby          = false;
    bool requestNormalOperation  = false;
    bool requestSensorError      = false;
};

/**
 * @brief Machine state and brewing
 */
struct MachineStateData {
    MachineStateId machineState        = MachineStateId::INIT;
    MachineStateId lastmachinestate    = MachineStateId::INIT;
    int            lastmachinestatepid = -1;
    bool           emergencyStop       = false;
    bool           steamON             = false;
    bool           steamFirstON        = false;
    bool           backflushOn         = false;
    int            currBackflushCycles = 1;
    bool           waterTankFull       = true;
    bool           systemInitialized   = false;

    MachineStateFlags flags = MachineStateFlags();

    hw_timer_t* timer = nullptr;
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
    String hotWaterStateDebug     = "off";
    String lastHotWaterStateDebug = "off";
};
} // namespace GlobalStateNamespace

/**
 * @brief Central global state container
 *
 * @deprecated This struct is deprecated and will be removed in Phase 2.
 *             Use SystemContext and coordinators instead.
 *
 * This struct contains all global state organized into logical groups.
 * Migration approach:
 * 1. Replace all extern declarations with access to this single struct
 * 2. Pass this struct (or parts of it) to managers via dependency injection
 * 3. Gradually move data ownership into individual managers
 * 4. Reduce this struct to only truly shared state
 */
DEPRECATED struct GlobalState {
    GlobalStateNamespace::ProcessState      process;
    GlobalStateNamespace::CoordinationState coordination;
    GlobalStateNamespace::HandlerRefs       handlers;
    GlobalStateNamespace::HardwareRefs      hardware;
    GlobalStateNamespace::NetworkState      network;
    GlobalStateNamespace::TimingState       timing;
    GlobalStateNamespace::StandbyState      standby;
    GlobalStateNamespace::SensorState       sensors;
    GlobalStateNamespace::MachineStateData  machine;
    GlobalStateNamespace::DisplayState      display;
    GlobalStateNamespace::DebugState        debug;

    // System-wide references (initialized later)
    Config* config = nullptr;
    PID*    pid    = nullptr;

    // System version info
    const char* sysVersion    = VERSION;
    String      systemVersion = String(VERSION);
};

// Handler initialization function
void initializeHandlers(CleverCoffee::SystemContext* systemContext = nullptr);
