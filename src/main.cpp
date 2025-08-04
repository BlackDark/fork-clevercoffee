/**
 * @file main.cpp
 *
 * @brief Main sketch
 *
 * @version 4.0.0 Master
 */

// STL includes
#include <map>
#include <memory>

// Libraries & Dependencies
#include "Logger.h"
#include "core/SystemInitializer.h"
#include "network/CleverCoffeeWiFiManager.h"
#include "network/MQTTManager.h"
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
#include "state/GlobalState.h"

// Utilities
#include "utils/Timer.h"
#include "utils/helperUtils.h"
#include "utils/legacyUtils.h"

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

#include "utils/timingDebug.h"
#include "isr.h"
#include "hardware/pressureSensor.h"
#include <Wire.h>

// System initializer
std::unique_ptr<SystemInitializer> systemInitializer = nullptr;

// Display Manager
#include "display/DisplayManager.h"
std::unique_ptr<DisplayManager> displayManager = nullptr;

// Hardware Manager
#include "hardware/HardwareManager.h"
std::unique_ptr<HardwareManager> hardwareManager = nullptr;


// Modern sensor management
#include "sensors/SensorManager.h"
SensorManager* sensorManager = nullptr;

// Modern state machine
#include "state/StateMachine.h"
std::unique_ptr<StateMachine> stateMachine = nullptr;

// Modern process control
#include "control/ProcessController.h"
std::unique_ptr<ProcessController> processController = nullptr;

// Modern UI management
#include "ui/UIManager.h"
std::unique_ptr<UIManager> uiManager = nullptr;

// Modern loop management
#include "core/LoopManager.h"
std::unique_ptr<LoopManager> loopManager = nullptr;

#include "standby.h"
#include "brewHandler.h"
#include "hotWaterHandler.h"
#include "embeddedWebserver.h"

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
char* number2string(double in);
char* number2string(float in);
char* number2string(int in);
char* number2string(unsigned int in);
float filterPressureValue(float input);
int writeSysParamsToMQTT(bool continueOnError);
void updateStandbyTimer();
void resetStandbyTimer();

void displayMessage(const String& text1, const String& text2, const String& text3, const String& text4, const String& text5, const String& text6);
void displayLogo(const String& displaymessagetext, const String& displaymessagetext2);
bool shouldDisplayBrewTimer();
void u8g2_prepare();

#include "display/displayTemplateManager.h"

#include "powerHandler.h"
#include "scaleHandler.h"
#include "steamHandler.h"

/**
 * @brief Check if Wifi is connected, if not reconnect abort function if offline, or brew is running
 */
void checkWifi() {
    static int wifiConnectCounter = 1;
    static bool wifiConnectedHandled = false;
    if (g_state.network.offlineMode || checkBrewActive()) return;

    // Try to connect and if it does not succeed, enter offline mode
    if ((millis() - g_state.network.lastWifiConnectionAttempt >= wifiConnectionDelay) && (g_state.network.wifiReconnects <= maxWifiReconnects)) {
        if (WiFi.status() != WL_CONNECTED) { // check WiFi connection status
            wifiConnectedHandled = false;

            if (wifiConnectCounter == 1) {
                g_state.network.wifiReconnects++;
                LOGF(INFO, "Attempting WIFI (re-)connection: %i", g_state.network.wifiReconnects);
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
                    g_state.network.lastWifiConnectionAttempt = millis();
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

    if (g_state.network.wifiReconnects >= maxWifiReconnects && WiFi.status() != WL_CONNECTED) {
        // no wifi connection after trying connection, initiate offline mode
        initOfflineMode();
    }
    else {
        if (WiFi.status() == WL_CONNECTED) {
            g_state.network.wifiReconnects = 0;
        }
    }
}

/**
 * Legacy machine state handler
 * @brief Handle the different states of the machine
 */
void handleMachineState() {
    switch (g_state.machine.machineState) {
        case kInit:
            if (!g_state.machine.waterTankFull) {
                g_state.machine.machineState = kWaterTankEmpty;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || (g_state.hardware.tempSensor != nullptr && g_state.hardware.tempSensor->hasError())) {
                g_state.machine.machineState = kSensorError;
            }

            if (!Config::getInstance().get<bool>("pid.enabled")) {
                g_state.machine.machineState = kPidDisabled;
            }
            else {
                g_state.machine.machineState = kPidNormal;
            }

            break;

        case kPidNormal:
            if (brew()) {
                g_state.machine.machineState = kBrew;

                if (Config::getInstance().get<bool>("standby.enabled")) {
                    resetStandbyTimer(g_state.machine.machineState);
                }
            }

            if (manualFlush()) {
                g_state.machine.machineState = kManualFlush;

                if (Config::getInstance().get<bool>("standby.enabled")) {
                    resetStandbyTimer(g_state.machine.machineState);
                }
            }

            if (g_state.machine.backflushOn) {
                g_state.machine.machineState = kBackflush;

                if (Config::getInstance().get<bool>("standby.enabled")) {
                    resetStandbyTimer(g_state.machine.machineState);
                }
            }

            if (g_state.machine.steamON) {
                g_state.machine.machineState = kSteam;

                if (Config::getInstance().get<bool>("standby.enabled")) {
                    resetStandbyTimer(g_state.machine.machineState);
                }
            }

            if (checkHotWaterStates()) {
                g_state.machine.machineState = kHotWater;

                if (Config::getInstance().get<bool>("standby.enabled")) {
                    resetStandbyTimer(g_state.machine.machineState);
                }
            }

            if (g_state.machine.emergencyStop) {
                g_state.machine.machineState = kEmergencyStop;
            }

            if (Config::getInstance().get<bool>("standby.enabled") && g_state.standby.standbyModeRemainingTimeMillis == 0) {
                g_state.machine.machineState = kStandby;
                setRuntimePidState(false);
            }

            if (!Config::getInstance().get<bool>("pid.enabled") && g_state.machine.machineState != kStandby) {
                g_state.machine.machineState = kPidDisabled;
            }

            if (!g_state.machine.waterTankFull) {
                g_state.machine.machineState = kWaterTankEmpty;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || (g_state.hardware.tempSensor != nullptr && g_state.hardware.tempSensor->hasError())) {
                g_state.machine.machineState = kSensorError;
            }

            break;

        case kBrew:
            if (!brew()) {
                g_state.machine.machineState = kPidNormal;
            }

            if (g_state.machine.emergencyStop) {
                g_state.machine.machineState = kEmergencyStop;
            }

            if (!Config::getInstance().get<bool>("pid.enabled")) {
                g_state.machine.machineState = kPidDisabled;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || (g_state.hardware.tempSensor != nullptr && g_state.hardware.tempSensor->hasError())) {
                g_state.machine.machineState = kSensorError;
            }

            if (g_state.machine.machineState != kBrew) {
                g_state.network.MQTTReCnctCount = 0; // allow MQTT to try to reconnect if exiting brew mode
            }

            break;

        case kManualFlush:
            if (!manualFlush()) {
                g_state.machine.machineState = kPidNormal;
            }

            if (g_state.machine.emergencyStop) {
                g_state.machine.machineState = kEmergencyStop;
            }

            if (!Config::getInstance().get<bool>("pid.enabled")) {
                g_state.machine.machineState = kPidDisabled;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || (g_state.hardware.tempSensor != nullptr && g_state.hardware.tempSensor->hasError())) {
                g_state.machine.machineState = kSensorError;
            }
            break;

        case kHotWater:
            if (!checkHotWaterStates()) {
                g_state.machine.machineState = kPidNormal;
            }

            if (g_state.machine.steamON) {
                g_state.machine.machineState = kSteam;

                if (Config::getInstance().get<bool>("standby.enabled")) {
                    resetStandbyTimer(g_state.machine.machineState);
                }
            }

            if (g_state.machine.emergencyStop) {
                g_state.machine.machineState = kEmergencyStop;
            }

            if (!Config::getInstance().get<bool>("pid.enabled")) {
                g_state.machine.machineState = kPidDisabled;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || (g_state.hardware.tempSensor != nullptr && g_state.hardware.tempSensor->hasError())) {
                g_state.machine.machineState = kSensorError;
            }

            break;

        case kSteam:
            if (!g_state.machine.steamON) {
                g_state.machine.machineState = kPidNormal;
            }

            if (g_state.machine.emergencyStop) {
                g_state.machine.machineState = kEmergencyStop;
            }

            if (Config::getInstance().get<bool>("pid.enabled") == 0) {
                g_state.machine.machineState = kPidDisabled;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || (g_state.hardware.tempSensor != nullptr && g_state.hardware.tempSensor->hasError())) {
                g_state.machine.machineState = kSensorError;
            }

            break;

        case kBackflush:
            backflush();

            if (!g_state.machine.backflushOn) {
                g_state.machine.machineState = kPidNormal;
            }

            if (g_state.machine.emergencyStop) {
                g_state.machine.machineState = kEmergencyStop;
            }

            if (!Config::getInstance().get<bool>("pid.enabled")) {
                g_state.machine.machineState = kPidDisabled;
            }

            if (!g_state.machine.waterTankFull && (g_state.sensors.currBackflushState == kBackflushIdle || g_state.sensors.currBackflushState == kBackflushFinished)) {
                g_state.machine.machineState = kWaterTankEmpty;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || (g_state.hardware.tempSensor != nullptr && g_state.hardware.tempSensor->hasError())) {
                g_state.machine.machineState = kSensorError;
            }

            break;

        case kEmergencyStop:
            if (!g_state.machine.emergencyStop) {
                g_state.machine.machineState = kPidNormal;
            }

            if (!Config::getInstance().get<bool>("pid.enabled")) {
                g_state.machine.machineState = kPidDisabled;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || (g_state.hardware.tempSensor != nullptr && g_state.hardware.tempSensor->hasError())) {
                g_state.machine.machineState = kSensorError;
            }

            break;

        case kWaterTankEmpty:
            if (g_state.machine.waterTankFull) {
                g_state.machine.machineState = kPidNormal;

                if (Config::getInstance().get<bool>("standby.enabled")) {
                    resetStandbyTimer(g_state.machine.machineState);
                }
            }

            if (!Config::getInstance().get<bool>("pid.enabled")) {
                g_state.machine.machineState = kPidDisabled;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || (g_state.hardware.tempSensor != nullptr && g_state.hardware.tempSensor->hasError())) {
                g_state.machine.machineState = kSensorError;
            }

            break;

        case kPidDisabled:
            if (Config::getInstance().get<bool>("pid.enabled")) {
                g_state.machine.machineState = kPidNormal;
            }

            if ((sensorManager != nullptr && sensorManager->hasSensorError()) || (g_state.hardware.tempSensor != nullptr && g_state.hardware.tempSensor->hasError())) {
                g_state.machine.machineState = kSensorError;
            }

            break;

        case kStandby:
            {
                bool oledEnabled = Config::getInstance().get<bool>("hardware.oled.enabled");

                if (g_state.standby.standbyModeRemainingTimeDisplayOffMillis == 0 && oledEnabled) {
                    g_state.hardware.display->setPowerSave(1);
                }

                if (Config::getInstance().get<bool>("pid.enabled")) {
                    g_state.machine.machineState = kPidNormal;
                    resetStandbyTimer(g_state.machine.machineState);

                    if (oledEnabled) {
                        g_state.hardware.display->setPowerSave(0);
                    }
                }
                if (g_state.machine.steamON) {
                    setRuntimePidState(true);
                    g_state.machine.machineState = kSteam;
                    resetStandbyTimer(g_state.machine.machineState);

                    if (oledEnabled) {
                        g_state.hardware.display->setPowerSave(0);
                    }
                }

                if (checkHotWaterStates()) {
                    setRuntimePidState(true);
                    g_state.machine.machineState = kHotWater;
                    resetStandbyTimer(g_state.machine.machineState);

                    if (oledEnabled) {
                        g_state.hardware.display->setPowerSave(0);
                    }
                }

                if (brew()) {
                    setRuntimePidState(true);
                    g_state.machine.machineState = kBrew;
                    resetStandbyTimer(g_state.machine.machineState);

                    if (oledEnabled) {
                        g_state.hardware.display->setPowerSave(0);
                    }
                }

                if (manualFlush()) {
                    setRuntimePidState(true);
                    g_state.machine.machineState = kManualFlush;
                    resetStandbyTimer(g_state.machine.machineState);

                    if (oledEnabled) {
                        g_state.hardware.display->setPowerSave(0);
                    }
                }

                if (g_state.machine.backflushOn) {
                    g_state.machine.machineState = kBackflush;
                    resetStandbyTimer(g_state.machine.machineState);

                    if (oledEnabled) {
                        g_state.hardware.display->setPowerSave(0);
                    }
                }

                if ((sensorManager != nullptr && sensorManager->hasSensorError()) || (g_state.hardware.tempSensor != nullptr && g_state.hardware.tempSensor->hasError())) {
                    if (oledEnabled) {
                        g_state.hardware.display->setPowerSave(0);
                    }

                    g_state.machine.machineState = kSensorError;
                }

                if (g_state.machine.machineState != kStandby) {
                    g_state.network.MQTTReCnctCount = 0; // allow MQTT to try to reconnect if exiting standby
                }

                break;
            }

        case kSensorError:
            g_state.machine.machineState = kSensorError;
            break;

        case kEepromError:
            g_state.machine.machineState = kEepromError;
            break;
    }

    if (g_state.machine.machineState != g_state.machine.lastmachinestate) {
        printMachineState();
        g_state.machine.lastmachinestate = g_state.machine.machineState;
    }
}

/**
 * @brief Set up internal WiFi hardware
 */
void wiFiSetup() {
    try {
        const bool oledEnabled = Config::getInstance().get<bool>("hardware.oled.enabled");

        // Don't pass display callback during system initialization - display isn't fully ready yet
        // TODO: Lost: User feedback during WiFi connection (no "Connecting to WiFi..." messages on display) with commit 271d43432fab22cc4e1c950ee107212886806b8f
        if (!g_state.network.cleverCoffeeWiFiManager->setupAndConnect(Config::getInstance().get<String>("system.hostname"), WIFI_PASSWORD, false, nullptr)) {
            g_state.network.offlineMode = true;
        }

        // Check if restart is required after AP configuration
        if (g_state.network.cleverCoffeeWiFiManager->requiresRestart()) {
            // Device will restart inside WiFiManager, this code may not be reached
        }

        LOG(INFO, "WiFi setup completed via WiFiManager");
    } catch (const std::exception& e) {
        LOG(ERROR, "Failed to initialize WiFiManager");
        g_state.network.offlineMode = true;
    }
}


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
        g_state.hardware.display = systemInitializer->getDisplayManager()->get();

        // Complete display initialization that requires global dependencies
        // This must be done AFTER SystemInitializer to avoid crashes during WiFi setup
        u8g2_prepare();
        initLangStrings();

        const int templateId = Config::getInstance().get<int>("display.template");
        DisplayTemplateManager::initializeDisplay(templateId);

        // Display logo using UIManager if available, fallback to direct call
        if (uiManager) {
            uiManager->displayLogo(String("Version "), g_state.systemVersion);
        }
        else {
            displayLogo(String("Version "), g_state.systemVersion);
        }
    }

    if (systemInitializer->getHardwareManager()) {
        HardwareManager* hwManager = systemInitializer->getHardwareManager();

        // Update compatibility pointers
        g_state.hardware.heaterRelay = &hwManager->getHeaterRelay();
        g_state.hardware.pumpRelay = &hwManager->getPumpRelay();
        g_state.hardware.valveRelay = &hwManager->getValveRelay();

        g_state.hardware.statusLed = hwManager->getStatusLed();
        g_state.hardware.brewLed = hwManager->getBrewLed();
        g_state.hardware.steamLed = hwManager->getSteamLed();

        // Legacy
        // powerSwitch = hwManager->getPowerSwitch();
        // brewSwitch = hwManager->getBrewSwitch();
        // steamSwitch = hwManager->getSteamSwitch();
        // hotWaterSwitch = hwManager->getHotWaterSwitch();
        // waterTankSensor = hwManager->getWaterTankSensor();

        // TODO Duplicate? -> SystemInitializer already sets these
        g_state.hardware.powerSwitch = hwManager->getPowerSwitch();
        g_state.hardware.brewSwitch = hwManager->getBrewSwitch();
        g_state.hardware.hotWaterSwitch = hwManager->getSteamSwitch();
        g_state.hardware.powerSwitch = hwManager->getHotWaterSwitch();
        g_state.hardware.waterTankSensor = hwManager->getWaterTankSensor();

        g_state.hardware.tempSensor = hwManager->getTempSensor();
    }

    // Get SensorManager reference from SystemInitializer
    sensorManager = systemInitializer->getSensorManager();

    // Complete initialization steps that require global dependencies
    if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled")) {
        if (sensorManager) {
            sensorManager->initializeScale();
        }
        else {
            initScale(); // Fallback to global function
        }
    }

    g_state.machine.systemInitialized = systemInitializer->isInitialized();

    // Initialize modern state machine after all managers are set up
    if (g_state.machine.systemInitialized) {
        stateMachine = std::make_unique<StateMachine>(systemInitializer->getDisplayManager(), systemInitializer->getHardwareManager(), sensorManager, systemInitializer->getWiFiManager(), systemInitializer->getMQTTManager());

        if (stateMachine->initialize()) {
            LOG(INFO, "StateMachine initialized successfully");
        }
        else {
            LOG(ERROR, "StateMachine initialization failed!");
        }

        // Initialize PID parameters now that config is available
        // TODO remove?
        g_state.process.aggbKi = (Config::getInstance().get<double>("pid.bd.tn") == 0) ? 0 : Config::getInstance().get<double>("pid.bd.kp") / Config::getInstance().get<double>("pid.bd.tn");
        g_state.process.aggbKd = Config::getInstance().get<double>("pid.bd.tv") * Config::getInstance().get<double>("pid.bd.kp");
        g_state.process.aggKi = (Config::getInstance().get<double>("pid.regular.tn") == 0) ? 0 : Config::getInstance().get<double>("pid.regular.kp") / Config::getInstance().get<double>("pid.regular.tn");
        g_state.process.aggKd = Config::getInstance().get<double>("pid.regular.tv") * Config::getInstance().get<double>("pid.regular.kp");

        // Set PID tunings now that parameters are calculated
        g_state.pid->SetTunings(Config::getInstance().get<double>("pid.regular.kp"), g_state.process.aggKi, g_state.process.aggKd, 1);

        // Initialize ProcessController for PID control
        processController = std::make_unique<ProcessController>(systemInitializer->getDisplayManager(), systemInitializer->getHardwareManager(), sensorManager, systemInitializer->getMQTTManager());

        if (processController->initialize()) {
            LOG(INFO, "ProcessController initialized successfully");
        }
        else {
            LOG(ERROR, "ProcessController initialization failed!");
        }

        // Initialize UIManager for display management
        uiManager = std::make_unique<UIManager>(systemInitializer->getDisplayManager());

        if (uiManager->initialize()) {
            LOG(INFO, "UIManager initialized successfully");
        }
        else {
            LOG(ERROR, "UIManager initialization failed!");
        }

        // Initialize LoopManager for main loop coordination
        loopManager = std::make_unique<LoopManager>(processController.get(), sensorManager, uiManager.get());

        if (loopManager->initialize()) {
            LOG(INFO, "LoopManager initialized successfully");
        }
        else {
            LOG(ERROR, "LoopManager initialization failed!");
        }

        // Fallback legacy
        if (!loopManager) {
            // Initialize timers after system initialization to avoid static initialization order fiasco
            g_state.timing.loopWaterTank = std::make_unique<Timer>(checkWaterTank, 200);
            g_state.timing.hassioDiscoveryTimer = std::make_unique<Timer>(sendHASSIODiscoveryMsg, 300000);
            g_state.timing.printDisplayTimer = std::make_unique<Timer>(DisplayTemplateManager::printScreen, 100);
        }
    }

    LOG(INFO, "System setup completed via SystemInitializer");
}

void loop() {
    if (loopManager) {
        // Use modern LoopManager for coordinated main loop updates
        loopManager->update();
    }
    else {
        // Fallback to legacy loop implementation
        // Accept potential connections for remote logging
        Logger::update();

        // Update water tank sensor
        if (g_state.timing.loopWaterTank) (*g_state.timing.loopWaterTank)();

        // Update PID settings & machine state
        loopPid();

        // Update LED output based on machine state
        loopLED();

        // print timing related data to check what is causing stutters
        debugTimingLoop();
    }
}

void loopPid() {

    // Update the temperature using ProcessController if available
    if (processController) {
        // Use ProcessController for temperature and PID management
        processController->updateProcessControl(static_cast<int>(g_state.machine.machineState));
    }
    else {
        // Fallback to original temperature reading logic
        if (sensorManager != nullptr) {
            // Update SensorManager first to get fresh readings
            sensorManager->update();

            // Use SensorManager for temperature reading (includes brew offset automatically)
            g_state.process.temperature = sensorManager->getCurrentTemperature();

            if (g_state.machine.machineState == kSteam) {
                // For steam mode, get raw temperature without brew offset
                if (g_state.hardware.tempSensor != nullptr) {
                    g_state.process.temperature = g_state.hardware.tempSensor->getCurrentTemperature();
                }
            }
        }
        else if (g_state.hardware.tempSensor != nullptr) {
            // Fallback to direct sensor access
            g_state.process.temperature = g_state.hardware.tempSensor->getCurrentTemperature();

            if (g_state.machine.machineState != kSteam) {
                g_state.process.temperature -= Config::getInstance().get<double>("brew.temp_offset");
            }
        }
    }

    // Reset temperature update flag after temperature reading is complete
    g_state.coordination.temperatureUpdateRunning = false;

    static bool wifiWasConnected = false;

    // Only do Wifi stuff, if Wifi is connected
    if (WiFi.status() == WL_CONNECTED && !g_state.network.offlineMode) {

        if (wifiWasConnected == false) {
            LOG(INFO, "WiFi Connected");
            wifiWasConnected = true;
        }

        if (g_state.network.mqttManager && g_state.network.mqttManager->isEnabled()) {
            g_state.network.mqttManager->setUpdateRunning(false);

            if (getSignalStrength() > 1) {
                g_state.network.mqttManager->checkConnection();

                // if screen is ready to refresh wait for next loop
                if (!g_state.coordination.displayBufferReady && !g_state.coordination.temperatureUpdateRunning) {
                    g_state.network.mqttManager->writeSysParamsToMQTT(true); // Continue on error
                }
            }

            g_state.coordination.hassioUpdateRunning = false;

            if (g_state.network.mqttManager->isConnected()) {
                g_state.network.mqttManager->loop();

                // resend discovery messages if not during a main function and MQTT has been disconnected but has now reconnected, or if last send failed
                if (!(g_state.machine.machineState >= kBrew && g_state.machine.machineState <= kBackflush) && ((!g_state.network.mqttManager->wasConnected() || g_state.network.hassioFailed) && !g_state.coordination.displayBufferReady && !g_state.coordination.temperatureUpdateRunning)) {
                    if (g_state.timing.hassioDiscoveryTimer) (*g_state.timing.hassioDiscoveryTimer)();
                }

                g_state.network.mqttManager->setWasConnected(true);
            }
            // Supress debug messages until we have a connection etablished
            else if (g_state.network.mqttManager->wasConnected()) {
                LOG(INFO, "MQTT disconnected");
                g_state.network.mqttManager->setWasConnected(false);
            }
        }

        ArduinoOTA.handle(); // For OTA

        // Disable interrupt if OTA is starting, otherwise it will not work
        ArduinoOTA.onStart([]() {
            disableTimer1();
            g_state.hardware.heaterRelay->off();
        });

        ArduinoOTA.onError([](ota_error_t error) { enableTimer1(); });

        // Enable interrupts if OTA is finished
        ArduinoOTA.onEnd([]() { enableTimer1(); });

        g_state.network.wifiReconnects = 0; // reset wifi reconnects if connected
    }
    else {
        wifiWasConnected = false;
        checkWifi();
    }

    // Emergency stop test and PID computation are now handled by ProcessController
    if (!processController) {
        // Fallback to original logic if ProcessController isn't available
        testEmergencyStop();    // test if temp is too high
        g_state.pid->Compute(); // the variable g_state.process.pidOutput now has new values from PID (will be written to heater pin in ISR.cpp)
    }

    if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled")) {
        checkWeight();    // Check Weight Scale in the loop
        shotTimerScale(); // Calculation of weight of shot while brew is running
    }

    if (Config::getInstance().get<bool>("hardware.sensors.pressure.enabled")) {
        if (sensorManager != nullptr) {
            // Pressure reading is handled by sensorManager->update() call above
            g_state.sensors.inputPressure = sensorManager->getCurrentPressure();
            g_state.sensors.inputPressureFilter = sensorManager->getFilteredPressure();
        }
        else {
            // Fallback to direct pressure reading
            if (const unsigned long currentMillisPressure = millis(); currentMillisPressure - g_state.timing.previousMillisPressure >= g_state.timing.intervalPressure) {
                g_state.timing.previousMillisPressure = currentMillisPressure;
                g_state.sensors.inputPressure = measurePressure();
                g_state.sensors.inputPressureFilter = filterPressureValue(g_state.sensors.inputPressure);
            }
        }
    }

    // refresh website if loop does not have anoth long running process already
    bool timeCondition = (millis() - g_state.network.lastTempEvent) > g_state.network.tempEventInterval;
    bool mqttCondition = (!g_state.network.mqttManager || !g_state.network.mqttManager->isUpdateRunning());
    bool hassioCondition = !g_state.coordination.hassioUpdateRunning;
    bool displayCondition = !g_state.coordination.displayBufferReady;
    bool tempCondition = !g_state.coordination.temperatureUpdateRunning;

    if (timeCondition && mqttCondition && hassioCondition && displayCondition && tempCondition) {
        g_state.coordination.websiteUpdateRunning = true;

        // send temperatures to website endpoint
        if (WiFi.status() == WL_CONNECTED && !g_state.network.offlineMode) {
            sendTempEvent(g_state.process.temperature, Config::getInstance().get<double>("brew.setpoint"), g_state.process.pidOutput / 10); // g_state.process.pidOutput is promill, so /10 to get percent value

            if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled")) {
                sendWeightEvent();
            }

            g_state.network.lastTempEvent = millis();
        }

        g_state.coordination.websiteUpdateRunning = false;

        // PID debug logging is now handled by ProcessController
        if (!processController && Config::getInstance().get<bool>("pid.enabled")) {
            // Fallback: Original PID debug logging
            LOGF(TRACE, "Current PID mode: %s", g_state.pid->GetPonE() ? "PonE" : "PonM");

            // P-Part
            LOGF(TRACE, "Current PID input error: %f", g_state.pid->GetInputError());
            LOGF(TRACE, "Current PID P part: %f", g_state.pid->GetLastPPart());
            LOGF(TRACE, "Current PID kP: %f", g_state.pid->GetKp());
            // I-Part
            LOGF(TRACE, "Current PID I sum: %f", g_state.pid->GetLastIPart());
            LOGF(TRACE, "Current PID kI: %f", g_state.pid->GetKi());
            // D-Part
            LOGF(TRACE, "Current PID diff'd input: %f", g_state.pid->GetDeltaInput());
            LOGF(TRACE, "Current PID D part: %f", g_state.pid->GetLastDPart());
            LOGF(TRACE, "Current PID kD: %f", g_state.pid->GetKd());
            // Combined PID output
            LOGF(TRACE, "Current PID Output: %f", g_state.process.pidOutput);
            LOGF(TRACE, "Current g_state.machine.machineState: %s", machinestateEnumToString(g_state.machine.machineState));
            // Brew
            LOGF(TRACE, "currBrewTime %f", g_state.process.currBrewTime);
            LOGF(TRACE, "Brew detected %i", checkBrewActive());
            LOGF(TRACE, "brewPidDisabled %i", g_state.process.brewPidDisabled);
        }
    }

    checkSteamSwitch();
    checkPowerSwitch();

    // Setpoint management is now handled by ProcessController
    if (!processController) {
        // Fallback: set setpoint depending on steam or brew mode
        if (g_state.machine.steamON == 1) {
            g_state.process.setpoint = Config::getInstance().get<double>("steam.setpoint");
        }
        else if (g_state.machine.steamON == 0) {
            g_state.process.setpoint = Config::getInstance().get<double>("brew.setpoint");
        }
    }

    updateStandbyTimer();

    // Update state machine (replaces handleMachineState())
    if (stateMachine && stateMachine->isInitialized()) {
        stateMachine->update();

        // Update compatibility variables for existing code
        const int newStateId = stateMachine->getCurrentStateId();
        if (newStateId != g_state.machine.machineState) {
            g_state.machine.lastmachinestate = static_cast<LegacyMachineState>(g_state.machine.machineState);
            g_state.machine.machineState = static_cast<LegacyMachineState>(newStateId);
            printMachineState();
        }
    }
    else {
        // Fallback to old state machine if new one isn't ready
        handleMachineState();
    }

    hotWaterHandler();
    valveSafetyShutdownCheck();

    if (Config::getInstance().get<bool>("hardware.switches.brew.enabled")) {
        // Update brew timer display state using UIManager if available
        if (uiManager) {
            uiManager->shouldDisplayBrewTimer();
        }
        else {
            shouldDisplayBrewTimer();
        }
    }

    // Display updates are now handled by UIManager
    static unsigned long lastDisplayDebugTime = 0;
    bool logDisplayDebug = (millis() - lastDisplayDebugTime > 3000);

    if (uiManager) {
        uiManager->setUpdateRunning(false);
        if (logDisplayDebug) LOGF(INFO, "Display: Using UIManager path");

        if (Config::getInstance().get<bool>("hardware.oled.enabled")) {
            bool websiteCondition = !g_state.coordination.websiteUpdateRunning;
            bool mqttCondition = (!g_state.network.mqttManager || !g_state.network.mqttManager->isUpdateRunning());
            bool hassioCondition = !g_state.coordination.hassioUpdateRunning;
            bool tempCondition = !g_state.coordination.temperatureUpdateRunning;
            bool standbyCondition = (!Config::getInstance().get<bool>("standby.enabled") || g_state.standby.standbyModeRemainingTimeMillis > 0);

            // update display on loops that have not had other major tasks running
            if (websiteCondition && mqttCondition && hassioCondition && tempCondition && standbyCondition) {
                if (uiManager->isBufferReady()) {
                    uiManager->forceUpdate();
                    uiManager->setBufferReady(false);
                    uiManager->setUpdateRunning(true);
                }
                else {
                    if (g_state.timing.printDisplayTimer) (*g_state.timing.printDisplayTimer)();
                }
            }
        }
    }
    else {
        // Fallback to original display logic
        g_state.coordination.displayUpdateRunning = false;

        if (Config::getInstance().get<bool>("hardware.oled.enabled")) {
            bool websiteCondition = !g_state.coordination.websiteUpdateRunning;
            bool mqttCondition = (!g_state.network.mqttManager || !g_state.network.mqttManager->isUpdateRunning());
            bool hassioCondition = !g_state.coordination.hassioUpdateRunning;
            bool tempCondition = !g_state.coordination.temperatureUpdateRunning;
            bool standbyCondition = (!Config::getInstance().get<bool>("standby.enabled") || g_state.standby.standbyModeRemainingTimeMillis > 0);

            if (websiteCondition && mqttCondition && hassioCondition && tempCondition && standbyCondition) {
                if (g_state.coordination.displayBufferReady) {
                    g_state.hardware.display->sendBuffer();
                    g_state.coordination.displayBufferReady = false;
                    g_state.coordination.displayUpdateRunning = true;
                }
                else {
                    if (g_state.timing.printDisplayTimer) (*g_state.timing.printDisplayTimer)();
                }
            }
        }
    }

    // PID state management and tuning is now handled by ProcessController
    if (!processController) {
        // Fallback: Original PID control logic

        // Check if PID should run or not. If not, set to manual and force output to zero
        if (g_state.machine.machineState == kPidDisabled || g_state.machine.machineState == kWaterTankEmpty || g_state.machine.machineState == kSensorError || g_state.machine.machineState == kEmergencyStop || g_state.machine.machineState == kEepromError || g_state.machine.machineState == kStandby ||
            g_state.machine.machineState == kBackflush || g_state.process.brewPidDisabled) {
            if (g_state.pid->GetMode() == 1) {
                // Force PID shutdown
                g_state.pid->SetMode(0);
                g_state.process.pidOutput = 0;
                g_state.hardware.heaterRelay->off();
            }
        }
        else { // no sensorerror, no pid off or no Emergency Stop
            if (g_state.pid->GetMode() == 0) {
                g_state.pid->SetMode(1);
            }
        }

        // Regular PID operation
        if (g_state.machine.machineState == kPidNormal) {
            setPIDTunings(Config::getInstance().get<bool>("pid.use_ponm"));
        }

        // Brew PID
        if (g_state.machine.machineState == kBrew) {
            if (Config::getInstance().get<double>("brew.pid_delay") > 0 && g_state.process.currBrewTime > 0 && g_state.process.currBrewTime < Config::getInstance().get<double>("brew.pid_delay") * 1000) {
                // disable PID for brewPidDelay seconds, enable PID again with new tunings after that
                if (!g_state.process.brewPidDisabled) {
                    g_state.process.brewPidDisabled = true;
                    g_state.pid->SetMode(MANUAL);
                    g_state.process.pidOutput = 0;
                    g_state.hardware.heaterRelay->off();
                    LOGF(DEBUG, "disabled PID, waiting for %.0f seconds before enabling PID again", Config::getInstance().get<double>("brew.pid_delay"));
                }
            }
            else {
                if (g_state.process.brewPidDisabled) {
                    // enable PID again
                    g_state.pid->SetMode(AUTOMATIC);
                    g_state.process.brewPidDisabled = false;
                    LOGF(DEBUG, "Enabled PID again after %.0f seconds of brew pid delay", Config::getInstance().get<double>("brew.pid_delay"));
                }

                if (Config::getInstance().get<bool>("pid.bd.enabled")) {
                    setBDPIDTunings();
                }
                else {
                    setPIDTunings(Config::getInstance().get<bool>("pid.use_ponm"));
                }
            }
        }
        // Reset brewPidDisabled if brew was aborted
        if (g_state.machine.machineState != kBrew && g_state.process.brewPidDisabled) {
            // enable PID again
            g_state.pid->SetMode(AUTOMATIC);
            g_state.process.brewPidDisabled = false;
            LOG(DEBUG, "Enabled PID again after brew was manually stopped");
        }

        // Steam on
        if (g_state.machine.machineState == kSteam) {
            if (g_state.machine.lastmachinestatepid != g_state.machine.machineState) {
                LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", 150.0, 0.0, 0.0);
                g_state.machine.lastmachinestatepid = g_state.machine.machineState;
            }

            g_state.pid->SetTunings(Config::getInstance().get<double>("pid.steam.kp"), 0, 0, 1);
        }
    }
}

void loopLED() {
    if (Config::getInstance().get<bool>("hardware.leds.status.enabled") && g_state.hardware.statusLed != nullptr) {
        if ((g_state.machine.machineState == kPidNormal && (fabs(g_state.process.temperature - g_state.process.setpoint) < 0.3)) || (g_state.process.temperature > 115 && fabs(g_state.process.temperature - g_state.process.setpoint) < 5)) {
            g_state.hardware.statusLed->turnOn();
        }
        else {
            g_state.hardware.statusLed->turnOff();
        }
    }

    if (Config::getInstance().get<bool>("hardware.leds.brew.enabled") && g_state.hardware.brewLed != nullptr) {
        g_state.hardware.brewLed->setGPIOState(g_state.machine.machineState == kBrew);
    }

    if (Config::getInstance().get<bool>("hardware.leds.steam.enabled") && g_state.hardware.steamLed != nullptr) {
        g_state.hardware.steamLed->setGPIOState(g_state.machine.machineState == kSteam);
    }
}

void checkWaterTank() {
    if (sensorManager != nullptr) {
        // Use SensorManager for water tank sensing
        // (sensor reading is updated by sensorManager->update() in loopPid)
        sensorManager->updateWaterTankSensor();
        g_state.machine.waterTankFull = sensorManager->isWaterTankFull();
    }
    else {
        // Fallback to direct water tank sensor reading
        if (!Config::getInstance().get<bool>("hardware.sensors.watertank.enabled") || g_state.hardware.waterTankSensor == nullptr) {
            return;
        }

        if (const bool isWaterDetected = g_state.hardware.waterTankSensor->isPressed(); isWaterDetected && !g_state.machine.waterTankFull) {
            g_state.machine.waterTankFull = true;
            LOG(INFO, "Water tank full");
        }
        else if (!isWaterDetected && g_state.machine.waterTankFull) {
            g_state.machine.waterTankFull = false;
            LOG(WARNING, "Water tank empty");
        }
    }
}


/**
 * @brief Safely shutdown machine operations
 * Turns off pump, valve, and heater regardless of current state
 */
void performSafeShutdown() {
    setRuntimePidState(false);

    g_state.hardware.heaterRelay->off();
    g_state.hardware.pumpRelay->off();
    g_state.hardware.valveRelay->off();

    // Reset all brew-related states
    if (g_state.sensors.currBrewState != kBrewIdle) {
        LOG(INFO, "Stopping active brew");
        g_state.sensors.currBrewState = kBrewIdle;
        g_state.sensors.currBrewSwitchState = kBrewSwitchIdle;
        g_state.process.currBrewTime = 0;
        g_state.process.startingTime = 0;
        g_state.sensors.brewSwitchWasOff = false;
    }

    // Reset manual flush states
    if (g_state.sensors.currManualFlushState != kManualFlushIdle) {
        LOG(INFO, "Stopping manual group head flush");
        g_state.sensors.currManualFlushState = kManualFlushIdle;
        g_state.sensors.currBrewSwitchState = kBrewSwitchIdle;
        g_state.process.currBrewTime = 0;
        g_state.process.startingTime = 0;
    }

    // Reset backflush state
    if (g_state.sensors.currBackflushState != kBackflushIdle) {
        LOG(INFO, "Stopping active backflush");
        g_state.sensors.currBackflushState = kBackflushIdle;
        g_state.machine.currBackflushCycles = 1;
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
    if (g_state.machine.steamON) {
        LOG(INFO, "Disabling steam mode");
        g_state.machine.steamON = false;
        g_state.machine.steamFirstON = false;
    }

    LOG(INFO, "Safe shutdown, all relays turned off");
}

void setPIDTunings(const bool usePonM) {
    // Prevent overwriting of brewdetection values
    // calc ki, kd
    if (Config::getInstance().get<double>("pid.regular.tn") != 0) {
        g_state.process.aggKi = Config::getInstance().get<double>("pid.regular.kp") / Config::getInstance().get<double>("pid.regular.tn");
    }
    else {
        g_state.process.aggKi = 0;
    }

    g_state.process.aggKd = Config::getInstance().get<double>("pid.regular.tv") * Config::getInstance().get<double>("pid.regular.kp");

    g_state.pid->SetIntegratorLimits(0, Config::getInstance().get<double>("pid.regular.i_max"));

    if (g_state.machine.lastmachinestatepid != g_state.machine.machineState) {
        LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", Config::getInstance().get<double>("pid.regular.kp"), g_state.process.aggKi, g_state.process.aggKd);
        g_state.machine.lastmachinestatepid = g_state.machine.machineState;
    }

    if (usePonM) {
        g_state.pid->SetTunings(Config::getInstance().get<double>("pid.bd.kp"), g_state.process.aggbKi, g_state.process.aggbKd, P_ON_M);
    }
    else {
        g_state.pid->SetTunings(Config::getInstance().get<double>("pid.regular.kp"), g_state.process.aggKi, g_state.process.aggKd, 1);
    }
}

void setBDPIDTunings() {
    // calc ki, kd
    if (Config::getInstance().get<double>("pid.bd.tn") != 0) {
        g_state.process.aggbKi = Config::getInstance().get<double>("pid.bd.kp") / Config::getInstance().get<double>("pid.bd.tn");
    }
    else {
        g_state.process.aggbKi = 0;
    }

    g_state.process.aggbKd = Config::getInstance().get<double>("pid.bd.tv") * Config::getInstance().get<double>("pid.bd.kp");

    if (g_state.machine.lastmachinestatepid != g_state.machine.machineState) {
        LOGF(DEBUG, "new PID-Values: P=%.1f  I=%.1f  D=%.1f", Config::getInstance().get<double>("pid.bd.kp"), g_state.process.aggbKi, g_state.process.aggbKd);
        g_state.machine.lastmachinestatepid = g_state.machine.machineState;
    }

    g_state.pid->SetTunings(Config::getInstance().get<double>("pid.bd.kp"), g_state.process.aggbKi, g_state.process.aggbKd, 1);
}
