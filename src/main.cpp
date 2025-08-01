/**
 * @file main.cpp
 *
 * @brief Main sketch
 *
 * @version 4.0.0 Master
 */

// STL includes
#include <map>

// Libraries & Dependencies
#include "Logger.h"
#include "core/SystemInitializer.h"
#include "network/MQTTManager.h"
#include "network/WiFiManager.h"
#include <ArduinoOTA.h>
#include <LittleFS.h>
#include <PID_v1.h>  // for PID calculation
#include <Preferences.h>
#include <U8g2lib.h> // i2c display
#include <WiFiManager.h>
#include <os.h>

// Defaults
#include "defaults.h"

// Includes
#include "Config.h"
#include "GlobalVariables.h"

// Utilities
#include "utils/Timer.h"

// Hardware classes
#include "hardware/GPIOPin.h"
#include "hardware/IOSwitch.h"
#include "hardware/LED.h"
#include "hardware/Relay.h"
#include "hardware/StandardLED.h"
#include "hardware/Switch.h"
#include "hardware/pinmapping.h"
#include "hardware/tempsensors/TempSensorDallas.h"
#include "hardware/tempsensors/TempSensorTSIC.h"

hw_timer_t* timer = nullptr;

#include "hardware/pressureSensor.h"
#include <Wire.h>

extern Config& config;

enum MachineState {
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
    kEepromError = 110,
};

MachineState machineState = kInit;
MachineState lastmachinestate = kInit;
int lastmachinestatepid = -1;

int displayOffline = 0;

inline bool systemInitialized = false;

// System initializer
std::unique_ptr<SystemInitializer> systemInitializer = nullptr;

// Display Manager
#include "display/DisplayManager.h"
std::unique_ptr<DisplayManager> displayManager = nullptr;

// Hardware Manager
#include "hardware/HardwareManager.h"
std::unique_ptr<HardwareManager> hardwareManager = nullptr;

// Compatibility function for existing code
U8G2* getU8G2() {
    return displayManager ? displayManager->get() : nullptr;
}

// Compatibility pointer for existing code - will be removed in future refactoring
U8G2* u8g2 = nullptr;

// Hardware compatibility pointers - will be removed in future refactoring
Switch* waterTankSensor = nullptr;
GPIOPin* statusLedPin = nullptr;
GPIOPin* brewLedPin = nullptr;
GPIOPin* steamLedPin = nullptr;
LED* statusLed = nullptr;
LED* brewLed = nullptr;
LED* steamLed = nullptr;
Relay* heaterRelay = nullptr;
Relay* pumpRelay = nullptr;
Relay* valveRelay = nullptr;
Switch* powerSwitch = nullptr;
Switch* brewSwitch = nullptr;
Switch* steamSwitch = nullptr;
Switch* hotWaterSwitch = nullptr;
TempSensor* tempSensor = nullptr;

// WiFi
// Modern WiFi and MQTT management
std::unique_ptr<CleverCoffeeWiFiManager> wifiManager = nullptr;
std::unique_ptr<MQTTManager> mqttManager = nullptr;

// Modern sensor management
#include "sensors/SensorManager.h"
SensorManager* sensorManager = nullptr;
constexpr unsigned long wifiConnectionDelay = WIFICONNECTIONDELAY;
constexpr unsigned int maxWifiReconnects = MAXWIFIRECONNECTS;
auto pass = WM_PASS;
unsigned long lastWifiConnectionAttempt = millis();
unsigned int wifiReconnects = 0; // actual number of reconnects
// offlineMode is defined in GlobalVariables.cpp
extern bool offlineMode;

// Helper function for timing debug
bool isMqttUpdateRunning() {
    return mqttManager && mqttManager->isUpdateRunning();
}

// Compatibility wrapper function
int writeSysParamsToMQTT(bool continueOnError = true) {
    if (mqttManager && mqttManager->isEnabled()) {
        return mqttManager->writeSysParamsToMQTT(continueOnError);
    }
    return 0;
}

// OTA
String otaPass;

// Pressure sensor
float inputPressure = 0;
float inputPressureFilter = 0;
const unsigned long intervalPressure = 100;
unsigned long previousMillisPressure; // initialisation at the end of init()

// timing flags
bool displayBufferReady = false;
bool displayUpdateRunning = false;
bool websiteUpdateRunning = false;
// MQTT update running is now managed by MQTTManager
bool hassioUpdateRunning = false;
bool temperatureUpdateRunning = false;

#include "utils/timingDebug.h"

#include "isr.h"

// Method forward declarations
void setSteamMode(const bool steamMode);
void setPIDTunings(bool usePonM);
void setBDPIDTunings();
void setRuntimePidState(bool enabled);
void loopcalibrate();
void loopPid();
void loopLED();
void checkWaterTank();
void printMachineState();
char const* machinestateEnumToString(MachineState machineState);
char* number2string(double in);
char* number2string(float in);
char* number2string(int in);
char* number2string(unsigned int in);
float filterPressureValue(float input);
int writeSysParamsToMQTT(bool continueOnError);
void updateStandbyTimer();
void resetStandbyTimer();
void wiFiReset();

// debugging water pump actions
String hotWaterStateDebug = "off";
String lastHotWaterStateDebug = "off";

// system parameters
double setpoint = brewSetpoint;

// PID - values for offline brew detection
double aggbKi = (aggbTn == 0) ? 0 : aggbKp / aggbTn;
double aggbKd = aggbTv * aggbKp;
double aggKi = (aggTn == 0) ? 0 : aggKp / aggTn;
double aggKd = aggTv * aggKp;

// Time PID will be disabled after brew started

#include "standby.h"

// Variables to hold PID values (Temp input, Heater output)
extern double temperature;
double pidOutput;
extern bool steamON;
bool steamFirstON = false;

PID bPID(&temperature, &pidOutput, &setpoint, aggKp, aggKi, aggKd, 1, DIRECT);

#include "brewHandler.h"
#include "hotWaterHandler.h"

// Other variables
boolean emergencyStop = false;                // Emergency stop if temperature is too high
constexpr double EmergencyStopTemp = 145;     // Temp EmergencyStopTemp
float inX = 0, inY = 0, inOld = 0, inSum = 0; // used for filterPressureValue()
boolean setupDone = false;

// Water tank sensor
boolean waterTankFull = true;
Timer loopWaterTank(&checkWaterTank, 200); // Check water tank level every 200 ms
int waterTankCheckConsecutiveReads = 0;    // Counter for consecutive readings of water tank sensor
constexpr int waterTankCountsNeeded = 3;   // Number of same readings to change water tank sensing

// PID controller
unsigned long previousMillistemp; // initialisation at the end of init()

double previousInput = 0;

// Embedded HTTP Server
#include "embeddedWebserver.h"

// cmp_str struct is now defined in MQTTManager.h

// MQTT
bool hassioFailed = false;
bool mqtt_was_connected = false;

// Compatibility variables for transition period
bool mqtt_enabled = false;
bool mqtt_hassio_enabled = false;
PubSubClient* mqtt = nullptr;
unsigned int MQTTReCnctCount = 0;
unsigned long previousMillisMQTT = 0;
unsigned long lastMQTTConnectionAttempt = 0;

// MQTT functionality is now managed by MQTTManager

unsigned long lastTempEvent = 0;
unsigned long tempEventInterval = 1000;

// MQTT discovery timer callback
void sendHASSIODiscoveryMsg() {
    if (mqttManager && mqttManager->isEnabled()) {
        mqttManager->sendHASSIODiscoveryMsg();
    }
}

Timer hassioDiscoveryTimer(&sendHASSIODiscoveryMsg, 300000);

/**
 * @brief Get Wifi signal strength and set signalBars for display
 */
int getSignalStrength() {
    if (offlineMode) return 0;

    long rssi;

    if (WiFi.status() == WL_CONNECTED) {
        rssi = WiFi.RSSI();
    }
    else {
        rssi = -100;
    }

    if (rssi >= -50) {
        return 4;
    }
    else if (rssi < -50 && rssi >= -65) {
        return 3;
    }
    else if (rssi < -65 && rssi >= -75) {
        return 2;
    }
    else if (rssi < -75 && rssi >= -80) {
        return 1;
    }

    return 0;
}

void displayMessage(const String& text1, const String& text2, const String& text3, const String& text4, const String& text5, const String& text6);
void displayLogo(const String& displaymessagetext, const String& displaymessagetext2);
bool shouldDisplayBrewTimer();
void u8g2_prepare();

#include "display/displayTemplateManager.h"

Timer printDisplayTimer(&DisplayTemplateManager::printScreen, 100);

#include "powerHandler.h"
#include "scaleHandler.h"
#include "steamHandler.h"

// Emergency stop if temp is too high
void testEmergencyStop() {
    if (temperature > EmergencyStopTemp && emergencyStop == false) {
        emergencyStop = true;
    }
    else if (temperature < (brewSetpoint + 5) && emergencyStop == true) {
        emergencyStop = false;
    }
}

/**
 * @brief Switch to offline mode if maxWifiReconnects were exceeded during boot
 */
void initOfflineMode() {
    if (Config::getInstance().get<bool>("hardware.oled.enabled")) {
        displayOffline = 1;
    }

    LOG(INFO, "Start offline mode with eeprom values, no wifi :(");
    offlineMode = true;
}

/**
 * @brief Check if Wifi is connected, if not reconnect abort function if offline, or brew is running
 */
void checkWifi() {
    static int wifiConnectCounter = 1;
    static bool wifiConnectedHandled = false;
    if (offlineMode || checkBrewActive()) return;

    // Try to connect and if it does not succeed, enter offline mode
    if ((millis() - lastWifiConnectionAttempt >= wifiConnectionDelay) && (wifiReconnects <= maxWifiReconnects)) {
        if (WiFi.status() != WL_CONNECTED) { // check WiFi connection status
            wifiConnectedHandled = false;

            if (wifiConnectCounter == 1) {
                wifiReconnects++;
                LOGF(INFO, "Attempting WIFI (re-)connection: %i", wifiReconnects);
                WiFi.disconnect();
                WiFi.begin();
            }

            delay(20);                // give WIFI some time to connect

            if (WiFi.status() != WL_CONNECTED && wifiConnectCounter < 100) {
                wifiConnectCounter++; // reconnect counter, maximum waiting time for reconnect = 20*100ms plus loop times
            }
            else {
                if (wifiConnectCounter == 100) {
                    LOGF(INFO, "Wifi Reconnection failed - %i loops", wifiConnectCounter);
                    lastWifiConnectionAttempt = millis();
                    wifiConnectCounter = 1;
                }
            }
        }
        else {
            if (wifiConnectedHandled == false) {
                LOGF(INFO, "Wifi Reconnected - %i loops", wifiConnectCounter);
                wifiConnectedHandled = true;
                wifiConnectCounter = 1;
            }
        }
    }

    if (wifiReconnects >= maxWifiReconnects && WiFi.status() != WL_CONNECTED) {
        // no wifi connection after trying connection, initiate offline mode
        initOfflineMode();
    }
    else {
        if (WiFi.status() == WL_CONNECTED) {
            wifiReconnects = 0;
        }
    }
}

char number2string_double[22];

char* number2string(const double in) {
    snprintf(number2string_double, sizeof(number2string_double), "%0.2f", in);

    return number2string_double;
}

char number2string_float[22];

char* number2string(const float in) {
    snprintf(number2string_float, sizeof(number2string_float), "%0.2f", in);

    return number2string_float;
}

char number2string_int[22];

char* number2string(const int in) {
    snprintf(number2string_int, sizeof(number2string_int), "%d", in);

    return number2string_int;
}

char number2string_uint[22];

char* number2string(const unsigned int in) {
    snprintf(number2string_uint, sizeof(number2string_uint), "%u", in);

    return number2string_uint;
}

/**
 * @brief Filter input value using exponential moving average filter (using fixed coefficients)
 *      After ~28 cycles the input is set to 99,66% if the real input value sum of inX and inY
 *      multiplier must be 1 increase inX multiplier to make the filter faster
 */
float filterPressureValue(const float input) {
    inX = static_cast<float>(input * 0.3);
    inY = static_cast<float>(inOld * 0.7);
    inSum = inX + inY;
    inOld = inSum;

    return inSum;
}

/**
 * @brief Handle the different states of the machine
 */
void handleMachineState() {
    switch (machineState) {
        case kInit:
            if (!waterTankFull) {
                machineState = kWaterTankEmpty;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || 
                (tempSensor != nullptr && tempSensor->hasError())) {
                machineState = kSensorError;
            }

            if (!pidON) {
                machineState = kPidDisabled;
            }
            else {
                machineState = kPidNormal;
            }

            break;

        case kPidNormal:
            if (brew()) {
                machineState = kBrew;

                if (standbyModeOn) {
                    resetStandbyTimer(machineState);
                }
            }

            if (manualFlush()) {
                machineState = kManualFlush;

                if (standbyModeOn) {
                    resetStandbyTimer(machineState);
                }
            }

            if (backflushOn) {
                machineState = kBackflush;

                if (standbyModeOn) {
                    resetStandbyTimer(machineState);
                }
            }

            if (steamON) {
                machineState = kSteam;

                if (standbyModeOn) {
                    resetStandbyTimer(machineState);
                }
            }

            if (checkHotWaterStates()) {
                machineState = kHotWater;

                if (standbyModeOn) {
                    resetStandbyTimer(machineState);
                }
            }

            if (emergencyStop) {
                machineState = kEmergencyStop;
            }

            if (standbyModeOn && standbyModeRemainingTimeMillis == 0) {
                machineState = kStandby;
                setRuntimePidState(false);
            }

            if (!pidON && machineState != kStandby) {
                machineState = kPidDisabled;
            }

            if (!waterTankFull) {
                machineState = kWaterTankEmpty;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || 
                (tempSensor != nullptr && tempSensor->hasError())) {
                machineState = kSensorError;
            }

            break;

        case kBrew:
            if (!brew()) {
                machineState = kPidNormal;
            }

            if (emergencyStop) {
                machineState = kEmergencyStop;
            }

            if (!pidON) {
                machineState = kPidDisabled;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || 
                (tempSensor != nullptr && tempSensor->hasError())) {
                machineState = kSensorError;
            }

            if (machineState != kBrew) {
                MQTTReCnctCount = 0; // allow MQTT to try to reconnect if exiting brew mode
            }

            break;

        case kManualFlush:
            if (!manualFlush()) {
                machineState = kPidNormal;
            }

            if (emergencyStop) {
                machineState = kEmergencyStop;
            }

            if (!pidON) {
                machineState = kPidDisabled;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || 
                (tempSensor != nullptr && tempSensor->hasError())) {
                machineState = kSensorError;
            }
            break;

        case kHotWater:
            if (!checkHotWaterStates()) {
                machineState = kPidNormal;
            }

            if (steamON) {
                machineState = kSteam;

                if (standbyModeOn) {
                    resetStandbyTimer(machineState);
                }
            }

            if (emergencyStop) {
                machineState = kEmergencyStop;
            }

            if (!pidON) {
                machineState = kPidDisabled;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || 
                (tempSensor != nullptr && tempSensor->hasError())) {
                machineState = kSensorError;
            }

            break;

        case kSteam:
            if (!steamON) {
                machineState = kPidNormal;
            }

            if (emergencyStop) {
                machineState = kEmergencyStop;
            }

            if (pidON == 0) {
                machineState = kPidDisabled;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || 
                (tempSensor != nullptr && tempSensor->hasError())) {
                machineState = kSensorError;
            }

            break;

        case kBackflush:
            backflush();

            if (!backflushOn) {
                machineState = kPidNormal;
            }

            if (emergencyStop) {
                machineState = kEmergencyStop;
            }

            if (!pidON) {
                machineState = kPidDisabled;
            }

            if (!waterTankFull && (currBackflushState == kBackflushIdle || currBackflushState == kBackflushFinished)) {
                machineState = kWaterTankEmpty;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || 
                (tempSensor != nullptr && tempSensor->hasError())) {
                machineState = kSensorError;
            }

            break;

        case kEmergencyStop:
            if (!emergencyStop) {
                machineState = kPidNormal;
            }

            if (!pidON) {
                machineState = kPidDisabled;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || 
                (tempSensor != nullptr && tempSensor->hasError())) {
                machineState = kSensorError;
            }

            break;

        case kWaterTankEmpty:
            if (waterTankFull) {
                machineState = kPidNormal;

                if (standbyModeOn) {
                    resetStandbyTimer(machineState);
                }
            }

            if (!pidON) {
                machineState = kPidDisabled;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || 
                (tempSensor != nullptr && tempSensor->hasError())) {
                machineState = kSensorError;
            }

            break;

        case kPidDisabled:
            if (pidON) {
                machineState = kPidNormal;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || 
                (tempSensor != nullptr && tempSensor->hasError())) {
                machineState = kSensorError;
            }

            break;

        case kStandby:
            {
                bool oledEnabled = Config::getInstance().get<bool>("hardware.oled.enabled");

                if (standbyModeRemainingTimeDisplayOffMillis == 0 && oledEnabled) {
                    u8g2->setPowerSave(1);
                }

                if (pidON) {
                    machineState = kPidNormal;
                    resetStandbyTimer(machineState);

                    if (oledEnabled) {
                        u8g2->setPowerSave(0);
                    }
                }
                if (steamON) {
                    setRuntimePidState(true);
                    machineState = kSteam;
                    resetStandbyTimer(machineState);

                    if (oledEnabled) {
                        u8g2->setPowerSave(0);
                    }
                }

                if (checkHotWaterStates()) {
                    setRuntimePidState(true);
                    machineState = kHotWater;
                    resetStandbyTimer(machineState);

                    if (oledEnabled) {
                        u8g2->setPowerSave(0);
                    }
                }

                if (brew()) {
                    setRuntimePidState(true);
                    machineState = kBrew;
                    resetStandbyTimer(machineState);

                    if (oledEnabled) {
                        u8g2->setPowerSave(0);
                    }
                }

                if (manualFlush()) {
                    setRuntimePidState(true);
                    machineState = kManualFlush;
                    resetStandbyTimer(machineState);

                    if (oledEnabled) {
                        u8g2->setPowerSave(0);
                    }
                }

                if (backflushOn) {
                    machineState = kBackflush;
                    resetStandbyTimer(machineState);

                    if (oledEnabled) {
                        u8g2->setPowerSave(0);
                    }
                }

                if ((sensorManager != nullptr && sensorManager->hasSensorError()) || 
                    (tempSensor != nullptr && tempSensor->hasError())) {
                    if (oledEnabled) {
                        u8g2->setPowerSave(0);
                    }

                    machineState = kSensorError;
                }

                if (machineState != kStandby) {
                    MQTTReCnctCount = 0; // allow MQTT to try to reconnect if exiting standby
                }

                break;
            }

        case kSensorError:
            machineState = kSensorError;
            break;

        case kEepromError:
            machineState = kEepromError;
            break;
    }

    if (machineState != lastmachinestate) {
        printMachineState();
        lastmachinestate = machineState;
    }
}

void printMachineState() {
    LOGF(DEBUG, "new machineState: %s -> %s", machinestateEnumToString(lastmachinestate), machinestateEnumToString(machineState));
}

char const* machinestateEnumToString(const MachineState machineState) {
    switch (machineState) {
        case kInit:
            return "Init";
        case kPidNormal:
            return "PID Normal";
        case kBrew:
            return "Brew";
        case kManualFlush:
            return "Manual Flush";
        case kHotWater:
            return "Hot Water";
        case kSteam:
            return "Steam";
        case kBackflush:
            return "Backflush";
        case kWaterTankEmpty:
            return "Water Tank Empty";
        case kEmergencyStop:
            return "Emergency Stop";
        case kPidDisabled:
            return "PID Disabled";
        case kStandby:
            return "Standby Mode";
        case kSensorError:
            return "Sensor Error";
        case kEepromError:
            return "EEPROM Error";
    }

    return "Unknown";
}

/**
 * @brief Set up internal WiFi hardware
 */
void wiFiSetup() {
    try {
        wifiManager = std::make_unique<CleverCoffeeWiFiManager>();
        const bool oledEnabled = Config::getInstance().get<bool>("hardware.oled.enabled");

        // Don't pass display callback during system initialization - display isn't fully ready yet
        // TODO: Lost: User feedback during WiFi connection (no "Connecting to WiFi..." messages on display) with commit 271d43432fab22cc4e1c950ee107212886806b8f
        if (!wifiManager->setupAndConnect(hostname, pass, false, nullptr)) {
            offlineMode = true;
        }

        // Check if restart is required after AP configuration
        if (wifiManager->requiresRestart()) {
            // Device will restart inside WiFiManager, this code may not be reached
        }

        LOG(INFO, "WiFi setup completed via WiFiManager");
    } catch (const std::exception& e) {
        LOG(ERROR, "Failed to initialize WiFiManager");
        offlineMode = true;
    }
}

void wiFiReset() {
    if (wifiManager) {
        wifiManager->resetSettings();
    }
    else {
        LOG(ERROR, "WiFiManager not initialized for reset");
        ESP.restart();
    }
}

extern const char sysVersion[];

void setup() {
    // Initialize system using RAII SystemInitializer
    systemInitializer = std::make_unique<SystemInitializer>();

    if (!systemInitializer->initialize()) {
        LOG(ERROR, "System initialization failed!");
        Serial.println("Critical system initialization error detected!");
        Serial.flush();
        exit(0);
    }

    // Update compatibility pointers from SystemInitializer
    if (systemInitializer->getDisplayManager()) {
        u8g2 = systemInitializer->getDisplayManager()->get();

        // Complete display initialization that requires global dependencies
        // This must be done AFTER SystemInitializer to avoid crashes during WiFi setup
        u8g2_prepare();
        initLangStrings(config);

        const int templateId = Config::getInstance().get<int>("display.template");
        DisplayTemplateManager::initializeDisplay(templateId);

        displayLogo(String("Version "), String(sysVersion));
    }

    if (systemInitializer->getHardwareManager()) {
        HardwareManager* hwManager = systemInitializer->getHardwareManager();

        // Update compatibility pointers
        heaterRelay = &hwManager->getHeaterRelay();
        pumpRelay = &hwManager->getPumpRelay();
        valveRelay = &hwManager->getValveRelay();

        statusLed = hwManager->getStatusLed();
        brewLed = hwManager->getBrewLed();
        steamLed = hwManager->getSteamLed();

        powerSwitch = hwManager->getPowerSwitch();
        brewSwitch = hwManager->getBrewSwitch();
        steamSwitch = hwManager->getSteamSwitch();
        hotWaterSwitch = hwManager->getHotWaterSwitch();
        waterTankSensor = hwManager->getWaterTankSensor();

        tempSensor = hwManager->getTempSensor();
    }

    // Get SensorManager reference from SystemInitializer
    sensorManager = systemInitializer->getSensorManager();

    // Complete initialization steps that require global dependencies
    if (config.get<bool>("hardware.sensors.scale.enabled")) {
        if (sensorManager) {
            sensorManager->initializeScale();
        } else {
            initScale(); // Fallback to global function
        }
    }

    systemInitialized = systemInitializer->isInitialized();

    LOG(INFO, "System setup completed via SystemInitializer");
}

void loop() {
    // Accept potential connections for remote logging
    Logger::update();

    // Update water tank sensor
    loopWaterTank();

    // Update PID settings & machine state
    loopPid();

    // Update LED output based on machine state
    loopLED();

    // print timing related data to check what is causing stutters
    debugTimingLoop();
}

void loopPid() {

    // Update the temperature:
    temperatureUpdateRunning = false;

    if (sensorManager != nullptr) {
        // Use SensorManager for temperature reading (includes brew offset automatically)
        temperature = sensorManager->getCurrentTemperature();

        if (machineState == kSteam) {
            // For steam mode, get raw temperature without brew offset
            if (tempSensor != nullptr) {
                temperature = tempSensor->getCurrentTemperature();
            }
        }
    } else if (tempSensor != nullptr) {
        // Fallback to direct sensor access
        temperature = tempSensor->getCurrentTemperature();

        if (machineState != kSteam) {
            temperature -= brewTempOffset;
        }
    }

    static bool wifiWasConnected = false;

    // Only do Wifi stuff, if Wifi is connected
    if (WiFi.status() == WL_CONNECTED && !offlineMode) {

        if (wifiWasConnected == false) {
            LOG(INFO, "WiFi Connected");
            wifiWasConnected = true;
        }

        if (mqttManager && mqttManager->isEnabled()) {
            mqttManager->setUpdateRunning(false);

            if (getSignalStrength() > 1) {
                mqttManager->checkConnection();

                // if screen is ready to refresh wait for next loop
                if (!displayBufferReady && !temperatureUpdateRunning) {
                    mqttManager->writeSysParamsToMQTT(true); // Continue on error
                }
            }

            hassioUpdateRunning = false;

            if (mqttManager->isConnected()) {
                mqttManager->loop();

                // resend discovery messages if not during a main function and MQTT has been disconnected but has now reconnected, or if last send failed
                if (!(machineState >= kBrew && machineState <= kBackflush) && ((!mqttManager->wasConnected() || hassioFailed) && !displayBufferReady && !temperatureUpdateRunning)) {
                    hassioDiscoveryTimer();
                }

                mqttManager->setWasConnected(true);
            }
            // Supress debug messages until we have a connection etablished
            else if (mqttManager->wasConnected()) {
                LOG(INFO, "MQTT disconnected");
                mqttManager->setWasConnected(false);
            }
        }

        ArduinoOTA.handle(); // For OTA

        // Disable interrupt if OTA is starting, otherwise it will not work
        ArduinoOTA.onStart([]() {
            disableTimer1();
            heaterRelay->off();
        });

        ArduinoOTA.onError([](ota_error_t error) { enableTimer1(); });

        // Enable interrupts if OTA is finished
        ArduinoOTA.onEnd([]() { enableTimer1(); });

        wifiReconnects = 0; // reset wifi reconnects if connected
    }
    else {
        wifiWasConnected = false;
        checkWifi();
    }

    testEmergencyStop(); // test if temp is too high
    bPID.Compute();      // the variable pidOutput now has new values from PID (will be written to heater pin in ISR.cpp)

    websiteUpdateRunning = false;

    if (config.get<bool>("hardware.sensors.scale.enabled")) {
        checkWeight();    // Check Weight Scale in the loop
        shotTimerScale(); // Calculation of weight of shot while brew is running
    }

    if (config.get<bool>("hardware.sensors.pressure.enabled")) {
        if (sensorManager != nullptr) {
            // Use SensorManager for pressure reading
            sensorManager->updatePressureSensor();
            inputPressure = sensorManager->getCurrentPressure();
            inputPressureFilter = sensorManager->getFilteredPressure();
        } else {
            // Fallback to direct pressure reading
            if (const unsigned long currentMillisPressure = millis(); currentMillisPressure - previousMillisPressure >= intervalPressure) {
                previousMillisPressure = currentMillisPressure;
                inputPressure = measurePressure();
                inputPressureFilter = filterPressureValue(inputPressure);
            }
        }
    }

    // refresh website if loop does not have anoth long running process already
    if (((millis() - lastTempEvent) > tempEventInterval) && ((!mqttManager || !mqttManager->isUpdateRunning()) && !hassioUpdateRunning && !displayBufferReady && !temperatureUpdateRunning)) {
        websiteUpdateRunning = true;

        // send temperatures to website endpoint
        if (WiFi.status() == WL_CONNECTED && !offlineMode) {
            sendTempEvent(temperature, brewSetpoint, pidOutput / 10); // pidOutput is promill, so /10 to get percent value

            if (config.get<bool>("hardware.sensors.scale.enabled")) {
                sendWeightEvent();
            }
        }

        lastTempEvent = millis();

        if (pidON) {
            LOGF(TRACE, "Current PID mode: %s", bPID.GetPonE() ? "PonE" : "PonM");

            // P-Part
            LOGF(TRACE, "Current PID input error: %f", bPID.GetInputError());
            LOGF(TRACE, "Current PID P part: %f", bPID.GetLastPPart());
            LOGF(TRACE, "Current PID kP: %f", bPID.GetKp());
            // I-Part
            LOGF(TRACE, "Current PID I sum: %f", bPID.GetLastIPart());
            LOGF(TRACE, "Current PID kI: %f", bPID.GetKi());
            // D-Part
            LOGF(TRACE, "Current PID diff'd input: %f", bPID.GetDeltaInput());
            LOGF(TRACE, "Current PID D part: %f", bPID.GetLastDPart());
            LOGF(TRACE, "Current PID kD: %f", bPID.GetKd());
            // Combined PID output
            LOGF(TRACE, "Current PID Output: %f", pidOutput);
            LOGF(TRACE, "Current Machinestate: %s", machinestateEnumToString(machineState));
            // Brew
            LOGF(TRACE, "currBrewTime %f", currBrewTime);
            LOGF(TRACE, "Brew detected %i", checkBrewActive());
            LOGF(TRACE, "brewPidDisabled %i", brewPidDisabled);
        }
    }

    checkSteamSwitch();
    checkPowerSwitch();

    // set setpoint depending on steam or brew mode
    if (steamON == 1) {
        setpoint = steamSetpoint;
    }
    else if (steamON == 0) {
        setpoint = brewSetpoint;
    }

    updateStandbyTimer();
    handleMachineState();
    hotWaterHandler();
    valveSafetyShutdownCheck();

    if (config.get<bool>("hardware.switches.brew.enabled")) {
        shouldDisplayBrewTimer();
    }

    displayUpdateRunning = false;

    if (config.get<bool>("hardware.oled.enabled")) {

        // update display on loops that have not had other major tasks running, if blocked it will send in the next loop (average 0.5ms)
        if (!websiteUpdateRunning && (!mqttManager || !mqttManager->isUpdateRunning()) && !hassioUpdateRunning && !temperatureUpdateRunning && (standbyModeRemainingTimeMillis > 0)) {

            // displayUpdateRunning currently doesn't block anything as it is near the end of the loop, but if this code block moves it can be used to block other processes
            // sendBuffer() takes around 35ms so it flags that it has happened
            if (displayBufferReady) {
                u8g2->sendBuffer();
                displayBufferReady = false;
                displayUpdateRunning = true;
            }
            else {
                printDisplayTimer();
            }
        }
    }

    // Check if PID should run or not. If not, set to manual and force output to zero
    if (machineState == kPidDisabled || machineState == kWaterTankEmpty || machineState == kSensorError || machineState == kEmergencyStop || machineState == kEepromError || machineState == kStandby ||
        machineState == kBackflush || brewPidDisabled) {
        if (bPID.GetMode() == 1) {
            // Force PID shutdown
            bPID.SetMode(0);
            pidOutput = 0;
            heaterRelay->off();
        }
    }
    else { // no sensorerror, no pid off or no Emergency Stop
        if (bPID.GetMode() == 0) {
            bPID.SetMode(1);
        }
    }

    // Regular PID operation
    if (machineState == kPidNormal) {
        setPIDTunings(usePonM);
    }

    // Brew PID
    if (machineState == kBrew) {
        if (brewPidDelay > 0 && currBrewTime > 0 && currBrewTime < brewPidDelay * 1000) {
            // disable PID for brewPidDelay seconds, enable PID again with new tunings after that
            if (!brewPidDisabled) {
                brewPidDisabled = true;
                bPID.SetMode(MANUAL);
                pidOutput = 0;
                heaterRelay->off();
                LOGF(DEBUG, "disabled PID, waiting for %.0f seconds before enabling PID again", brewPidDelay);
            }
        }
        else {
            if (brewPidDisabled) {
                // enable PID again
                bPID.SetMode(AUTOMATIC);
                brewPidDisabled = false;
                LOGF(DEBUG, "Enabled PID again after %.0f seconds of brew pid delay", brewPidDelay);
            }

            if (useBDPID) {
                setBDPIDTunings();
            }
            else {
                setPIDTunings(usePonM);
            }
        }
    }
    // Reset brewPidDisabled if brew was aborted
    if (machineState != kBrew && brewPidDisabled) {
        // enable PID again
        bPID.SetMode(AUTOMATIC);
        brewPidDisabled = false;
        LOG(DEBUG, "Enabled PID again after brew was manually stopped");
    }

    // Steam on
    if (machineState == kSteam) {
        if (lastmachinestatepid != machineState) {
            LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", 150.0, 0.0, 0.0);
            lastmachinestatepid = machineState;
        }

        bPID.SetTunings(steamKp, 0, 0, 1);
    }
}

void loopLED() {
    if (config.get<bool>("hardware.leds.status.enabled") && statusLed != nullptr) {
        if ((machineState == kPidNormal && (fabs(temperature - setpoint) < 0.3)) || (temperature > 115 && fabs(temperature - setpoint) < 5)) {
            statusLed->turnOn();
        }
        else {
            statusLed->turnOff();
        }
    }

    if (config.get<bool>("hardware.leds.brew.enabled") && brewLed != nullptr) {
        brewLed->setGPIOState(machineState == kBrew);
    }

    if (config.get<bool>("hardware.leds.steam.enabled") && steamLed != nullptr) {
        steamLed->setGPIOState(machineState == kSteam);
    }
}

void checkWaterTank() {
    if (sensorManager != nullptr) {
        // Use SensorManager for water tank sensing
        sensorManager->updateWaterTankSensor();
        waterTankFull = sensorManager->isWaterTankFull();
    } else {
        // Fallback to direct water tank sensor reading
        if (!config.get<bool>("hardware.sensors.watertank.enabled") || waterTankSensor == nullptr) {
            return;
        }

        if (const bool isWaterDetected = waterTankSensor->isPressed(); isWaterDetected && !waterTankFull) {
            waterTankFull = true;
            LOG(INFO, "Water tank full");
        }
        else if (!isWaterDetected && waterTankFull) {
            waterTankFull = false;
            LOG(WARNING, "Water tank empty");
        }
    }
}

void setRuntimePidState(const bool enabled) {
    pidON = enabled ? 1 : 0;
    // Update via config system
    Config::getInstance().set<bool>("pid.enabled", enabled);
}

void setSteamMode(const bool steamMode) {
    steamON = steamMode;

    if (steamON) {
        steamFirstON = true;
    }

    if (!steamON) {
        steamFirstON = false;
    }
}

/**
 * @brief Safely shutdown machine operations
 * Turns off pump, valve, and heater regardless of current state
 */
void performSafeShutdown() {
    setRuntimePidState(false);

    heaterRelay->off();
    pumpRelay->off();
    valveRelay->off();

    // Reset all brew-related states
    if (currBrewState != kBrewIdle) {
        LOG(INFO, "Stopping active brew");
        currBrewState = kBrewIdle;
        currBrewSwitchState = kBrewSwitchIdle;
        currBrewTime = 0;
        startingTime = 0;
        brewSwitchWasOff = false;
    }

    // Reset manual flush states
    if (currManualFlushState != kManualFlushIdle) {
        LOG(INFO, "Stopping manual group head flush");
        currManualFlushState = kManualFlushIdle;
        currBrewSwitchState = kBrewSwitchIdle;
        currBrewTime = 0;
        startingTime = 0;
    }

    // Reset backflush state
    if (currBackflushState != kBackflushIdle) {
        LOG(INFO, "Stopping active backflush");
        currBackflushState = kBackflushIdle;
        currBackflushCycles = 1;
    }

    // Reset hot water state
    if (currHotWaterState != kHotWaterIdle) {
        LOG(INFO, "Stopping hot water draw");
        currHotWaterState = kHotWaterIdle;
        currHotWaterSwitchState = kHotWaterSwitchIdle;
        currPumpOnTime = 0;
        pumpStartingTime = 0;
    }

    // Turn off steam mode if active
    if (steamON) {
        LOG(INFO, "Disabling steam mode");
        steamON = false;
        steamFirstON = false;
    }

    LOG(INFO, "Safe shutdown, all relays turned off");
}

void setPIDTunings(const bool usePonM) {
    // Prevent overwriting of brewdetection values
    // calc ki, kd
    if (aggTn != 0) {
        aggKi = aggKp / aggTn;
    }
    else {
        aggKi = 0;
    }

    aggKd = aggTv * aggKp;

    bPID.SetIntegratorLimits(0, aggIMax);

    if (lastmachinestatepid != machineState) {
        LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggKp, aggKi, aggKd);
        lastmachinestatepid = machineState;
    }

    if (usePonM) {
        bPID.SetTunings(aggbKp, aggbKi, aggbKd, P_ON_M);
    }
    else {
        bPID.SetTunings(aggKp, aggKi, aggKd, 1);
    }
}

void setBDPIDTunings() {
    // calc ki, kd
    if (aggbTn != 0) {
        aggbKi = aggbKp / aggbTn;
    }
    else {
        aggbKi = 0;
    }

    aggbKd = aggbTv * aggbKp;

    if (lastmachinestatepid != machineState) {
        LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", aggbKp, aggbKi, aggbKd);
        lastmachinestatepid = machineState;
    }

    bPID.SetTunings(aggbKp, aggbKi, aggbKd, 1);
}
