/**
 * @file LoopManager.cpp
 * @brief Implementation of LoopManager for main loop coordination
 */

#include "LoopManager.h"
#include "../Config.h"
#include "../control/ProcessController.h"
#include "../hardware/Relay.h"
#include "../network/MQTTManager.h"
#include "../sensors/SensorManager.h"
#include "../state/StateMachine.h"
#include "../ui/UIManager.h"
#include "../utils/Timer.h"
#include "Logger.h"
#include <Arduino.h>
#include <ArduinoOTA.h>
#include <WiFi.h>

// Forward declaration for debug timing function
extern void debugTimingLoop();

// Forward declaration for display template function
namespace DisplayTemplateManager {
    extern void printScreen();
}

// Forward declarations for external variables needed for display updates
extern U8G2* u8g2;

// Timer for display updates is stored in g_state.timing.printDisplayTimer

// External managers
extern std::unique_ptr<MQTTManager> mqttManager;

// External standby variables
extern unsigned long standbyModeRemainingTimeMillis;

// Machine state enum - should be moved to a header later
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
    kEepromError = 110,
};

// External function declarations
extern bool checkBrewActive();
extern void checkWaterTank();
extern void sendHASSIODiscoveryMsg();

extern bool waterTankFull;
extern LegacyMachineState machineState;
// temperature moved to g_state.process.temperature
// setpoint moved to g_state.process.setpoint

// Display-related external declarations
extern unsigned long standbyModeRemainingTimeMillis;
extern U8G2* u8g2;
extern std::unique_ptr<MQTTManager> mqttManager;

// Hardware components for LED control
extern std::unique_ptr<Relay> statusLed;
extern std::unique_ptr<Relay> brewLed;
extern std::unique_ptr<Relay> steamLed;

// Hardware components for heating
extern Relay* heaterRelay;

LoopManager::LoopManager(ProcessController* processController, SensorManager* sensorManager, UIManager* uiManager) :
    processController_(processController),
    sensorManager_(sensorManager),
    uiManager_(uiManager),
    initialized_(false),
    waterTankTimerInitialized_(false),
    performanceMonitoringEnabled_(false),
    lastLoopTime_(0),
    maxLoopTime_(0),
    loopCount_(0) {

    LOG(INFO, "LoopManager created");
}

bool LoopManager::initialize() {
    LOG(INFO, "Initializing LoopManager");

    // Setup water tank monitoring
    if (!setupWaterTankTimer()) {
        LOG(WARNING, "LoopManager: Water tank timer setup failed");
        // Continue initialization - this is not critical
    }

    if (!setupTimers()) {
        LOG(ERROR, "LoopManager: Timer setup failed");
        return false;
    }

    // Enable performance monitoring for debugging
    performanceMonitoringEnabled_ = true;

    // Initialize timing variables
    lastLoopTime_ = millis();
    maxLoopTime_ = 0;
    loopCount_ = 0;

    initialized_ = true;

    LOG(INFO, "LoopManager initialized successfully");
    return true;
}

void LoopManager::update() {
    if (!initialized_) {
        LOG(WARNING, "LoopManager::update() called but not initialized");
        return;
    }

    // Performance timing start
    unsigned long loopStartTime = millis();

    // 1. Accept potential connections for remote logging
    Logger::update();

    // 2. Update water tank sensor (timer-based, 200ms intervals)
    updateWaterTank();

    // 3. Update PID settings & machine state
    updateProcessControl();

    // 4. Update LED output based on machine state
    updateLEDs();

    // 5. Update network (WiFi/MQTT/OTA)
    updateNetwork();

    // 6. Update website and data transmission
    updateWebsite();

    // 7. Update sensors (scale, pressure)
    updateSensors();

    // 8. Update switches and standby management
    updateSwitchesAndStandby();

    // 9. Update state machine
    updateStateMachine();

    // 10. Update display (critical for screen refresh)
    updateDisplay();

    // 11. Print timing related data to check what is causing stutters
    updateDebugTiming();

    // Performance timing end
    if (performanceMonitoringEnabled_) {
        unsigned long loopDuration = millis() - loopStartTime;
        if (loopDuration > maxLoopTime_) {
            maxLoopTime_ = loopDuration;
        }
        loopCount_++;

        // Log performance statistics every 1000 loops
        if (loopCount_ % 1000 == 0) {
            LOGF(DEBUG, "LoopManager: Processed %lu loops, max duration: %lums", loopCount_, maxLoopTime_);
        }
    }
}

void LoopManager::updateLEDs() {
    // Status LED - indicates when temperature is reached
    if (Config::getInstance().get<bool>("hardware.leds.status.enabled") && statusLed != nullptr) {
        bool shouldTurnOn = false;

        // Turn on when at target temperature (normal or steam mode)
        if ((machineState == kPidNormal && (fabs(g_state.process.temperature - g_state.process.setpoint) < 0.3)) || (g_state.process.temperature > 115 && fabs(g_state.process.temperature - g_state.process.setpoint) < 5)) {
            shouldTurnOn = true;
        }

        if (shouldTurnOn) {
            statusLed->on();
        }
        else {
            statusLed->off();
        }
    }

    // Brew LED - indicates brewing state
    if (Config::getInstance().get<bool>("hardware.leds.brew.enabled") && brewLed != nullptr) {
        if (machineState == kBrew) {
            brewLed->on();
        }
        else {
            brewLed->off();
        }
    }

    // Steam LED - indicates steam mode
    if (Config::getInstance().get<bool>("hardware.leds.steam.enabled") && steamLed != nullptr) {
        if (machineState == kSteam) {
            steamLed->on();
        }
        else {
            steamLed->off();
        }
    }
}

void LoopManager::updateWaterTank() {
    // Water tank monitoring is handled by the timer-based system
    // The timer calls checkWaterTank() every 200ms automatically
    if (waterTankTimer_) {
        // Advance the timer so checkWaterTank() is called at the correct interval
        (*waterTankTimer_)();
    }
    else {
        // Fallback: direct call to water tank check
        // This should normally not be needed if timer is set up correctly
        checkWaterTank();
    }
}

void LoopManager::updateProcessControl() {
    if (processController_) {
        // Use modern ProcessController for PID and temperature management
        processController_->updateProcessControl(machineState, false); // TODO: Get brewPidDisabled from proper source
    }
    else {
        // Fallback to original temperature reading logic - handled in main.cpp for now
        LOG(DEBUG, "LoopManager: ProcessController not available, using fallback");
    }
}

void LoopManager::updateDisplay() {
    // Handle display updates similar to the original main loop logic
    if (uiManager_) {
        // Use UIManager for display management
        LOGF(DEBUG, "LoopManager: Using UIManager path for display updates");
        uiManager_->setUpdateRunning(false);

        if (Config::getInstance().get<bool>("hardware.oled.enabled")) {
            // Only update display on loops that have not had other major tasks running
            // and when not in standby display-off mode
            bool websiteCondition = !g_state.coordination.websiteUpdateRunning;
            bool mqttCondition = (!mqttManager || !mqttManager->isUpdateRunning());
            bool hassioCondition = !g_state.coordination.hassioUpdateRunning;
            bool tempCondition = !g_state.coordination.temperatureUpdateRunning;
            bool standbyCondition = (!Config::getInstance().get<bool>("standby.enabled") || standbyModeRemainingTimeMillis > 0);

            // update display on loops that have not had other major tasks running
            if (websiteCondition && mqttCondition && hassioCondition && tempCondition && standbyCondition) {
                if (uiManager_->isBufferReady()) {
                    uiManager_->forceUpdate();
                    uiManager_->setBufferReady(false);
                    uiManager_->setUpdateRunning(true);
                }
                else {
                    // This is the critical call that was missing!
                    // It triggers the display template rendering
                    if (g_state.timing.printDisplayTimer) {
                        LOGF(DEBUG, "LoopManager: Calling printDisplayTimer (UIManager path)");
                        (*g_state.timing.printDisplayTimer)();
                    }
                    else {
                        LOGF(WARNING, "LoopManager: printDisplayTimer is null!");
                    }
                }
            }
        }
    }
    else {
        // Fallback to original display logic when UIManager is not available
        LOGF(DEBUG, "LoopManager: Using fallback path for display updates (no UIManager)");
        g_state.coordination.displayUpdateRunning = false;

        if (Config::getInstance().get<bool>("hardware.oled.enabled")) {
            // Only update display on loops that have not had other major tasks running
            // and when not in standby display-off mode
            bool websiteCondition = !g_state.coordination.websiteUpdateRunning;
            bool mqttCondition = (!mqttManager || !mqttManager->isUpdateRunning());
            bool hassioCondition = !g_state.coordination.hassioUpdateRunning;
            bool tempCondition = !g_state.coordination.temperatureUpdateRunning;
            bool standbyCondition = (!Config::getInstance().get<bool>("standby.enabled") || standbyModeRemainingTimeMillis > 0);

            // update display on loops that have not had other major tasks running
            if (websiteCondition && mqttCondition && hassioCondition && tempCondition && standbyCondition) {
                if (g_state.coordination.displayBufferReady) {
                    u8g2->sendBuffer();
                    g_state.coordination.displayBufferReady = false;
                    g_state.coordination.displayUpdateRunning = true;
                }
                else {
                    // This is the critical call that was missing!
                    // It triggers the display template rendering which sets displayBufferReady = true
                    if (g_state.timing.printDisplayTimer) {
                        LOGF(DEBUG, "LoopManager: Calling printDisplayTimer (fallback path)");
                        (*g_state.timing.printDisplayTimer)();
                    }
                    else {
                        LOGF(WARNING, "LoopManager: printDisplayTimer is null in fallback!");
                    }
                }
            }
        }
    }
}

void LoopManager::updateDebugTiming() {
    // Call the global debug timing function
    // This monitors loop performance and logs slow operations
    debugTimingLoop();
}

bool LoopManager::setupTimers() {
    try {
        g_state.timing.loopWaterTank2 = new Timer(checkWaterTank, 200);
        g_state.timing.hassioDiscoveryTimer2 = new Timer(sendHASSIODiscoveryMsg, 300000);
        g_state.timing.printDisplayTimer2 = new Timer(DisplayTemplateManager::printScreen, 100);

        g_state.timing.loopWaterTank = std::make_unique<Timer>(&checkWaterTank, 200);
        g_state.timing.hassioDiscoveryTimer = std::make_unique<Timer>(&sendHASSIODiscoveryMsg, 300000);
        g_state.timing.printDisplayTimer = std::make_unique<Timer>(DisplayTemplateManager::printScreen, 100);

        LOG(INFO, "LoopManager: Initialized timers");
        return true;
    } catch (const std::exception& e) {
        LOGF(ERROR, "LoopManager: Exception creating timers: %s", e.what());
        return false;
    }
}

bool LoopManager::setupWaterTankTimer() {
    try {
        // Create timer for water tank monitoring (200ms interval)
        waterTankTimer_ = std::make_unique<Timer>(&checkWaterTank, 200);

        if (!waterTankTimer_) {
            LOG(ERROR, "LoopManager: Failed to create water tank timer");
            return false;
        }

        waterTankTimerInitialized_ = true;
        LOG(INFO, "LoopManager: Water tank timer initialized (200ms interval)");
        return true;
    } catch (const std::exception& e) {
        LOGF(ERROR, "LoopManager: Exception creating water tank timer: %s", e.what());
        return false;
    }
}

void LoopManager::checkWaterTankLevel() {
    // This method provides a member function interface to the global checkWaterTank
    // for potential future encapsulation improvements
    checkWaterTank();
}

bool LoopManager::getPerformanceStats() const {
    return performanceMonitoringEnabled_ && loopCount_ > 0;
}

void LoopManager::updateNetwork() {
    // Network management (WiFi/MQTT/OTA) extracted from loopPid()
    static bool wifiWasConnected = false;
    extern void checkWifi();
    extern int getSignalStrength();
    extern bool hassioFailed;
    extern unsigned int wifiReconnects;

    // Only do Wifi stuff, if Wifi is connected
    if (WiFi.status() == WL_CONNECTED && !g_state.network.offlineMode) {
        if (wifiWasConnected == false) {
            LOG(INFO, "WiFi Connected");
            wifiWasConnected = true;
        }

        if (mqttManager && mqttManager->isEnabled()) {
            mqttManager->setUpdateRunning(false);

            if (getSignalStrength() > 1) {
                mqttManager->checkConnection();

                // if screen is ready to refresh wait for next loop
                if (!g_state.coordination.displayBufferReady && !g_state.coordination.temperatureUpdateRunning) {
                    mqttManager->writeSysParamsToMQTT(true);
                }
            }

            g_state.coordination.hassioUpdateRunning = false;

            if (mqttManager->isConnected()) {
                mqttManager->loop();

                // resend discovery messages if not during a main function and MQTT has been disconnected but has now reconnected
                if (!(machineState >= kBrew && machineState <= kBackflush) && ((!mqttManager->wasConnected() || hassioFailed) && !g_state.coordination.displayBufferReady && !g_state.coordination.temperatureUpdateRunning)) {
                    if (g_state.timing.hassioDiscoveryTimer) (*g_state.timing.hassioDiscoveryTimer)();
                }

                mqttManager->setWasConnected(true);
            }
            else if (mqttManager->wasConnected()) {
                LOG(INFO, "MQTT disconnected");
                mqttManager->setWasConnected(false);
            }
        }

        // OTA handling
        ArduinoOTA.handle();
        extern void disableTimer1();
        extern void enableTimer1();

        // Disable interrupt if OTA is starting, otherwise it will not work
        ArduinoOTA.onStart([]() {
            disableTimer1();
            if (heaterRelay) heaterRelay->off();
        });

        ArduinoOTA.onError([](ota_error_t error) { enableTimer1(); });
        ArduinoOTA.onEnd([]() { enableTimer1(); });

        wifiReconnects = 0; // reset wifi reconnects if connected
    }
    else {
        wifiWasConnected = false;
        checkWifi();
    }
}

void LoopManager::updateWebsite() {
    // Website and data transmission updates extracted from loopPid()
    extern void sendTempEvent(double temp, double setpoint, double pidOutput);
    extern void sendWeightEvent();
    // pidOutput moved to g_state.process.pidOutput

    bool timeCondition = (millis() - g_state.network.lastTempEvent) > g_state.network.tempEventInterval;
    bool mqttCondition = (!mqttManager || !mqttManager->isUpdateRunning());
    bool hassioCondition = !g_state.coordination.hassioUpdateRunning;
    bool displayCondition = !g_state.coordination.displayBufferReady;
    bool tempCondition = !g_state.coordination.temperatureUpdateRunning;

    if (timeCondition && mqttCondition && hassioCondition && displayCondition && tempCondition) {
        LOGF(DEBUG, "LoopManager: Conditions met for sending temperature events");
        g_state.coordination.websiteUpdateRunning = true;

        // send temperatures to website endpoint
        if (WiFi.status() == WL_CONNECTED && !g_state.network.offlineMode) {
            LOGF(DEBUG, "LoopManager: Sending temperature event: temp=%.2f, setpoint=%.2f, output=%.2f", g_state.process.temperature, Config::getInstance().get<double>("brew.setpoint"), g_state.process.pidOutput / 10);
            sendTempEvent(g_state.process.temperature, Config::getInstance().get<double>("brew.setpoint"), g_state.process.pidOutput / 10); // pidOutput is promill, so /10 to get percent value

            if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled")) {
                sendWeightEvent();
            }
            g_state.network.lastTempEvent = millis();
        }

        g_state.coordination.websiteUpdateRunning = false;
    }
}

void LoopManager::updateSensors() {
    // Scale and pressure sensor updates extracted from loopPid()
    extern void checkWeight();
    extern void shotTimerScale();
    extern float measurePressure();
    extern float filterPressureValue(float input);

    if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled")) {
        checkWeight();    // Check Weight Scale in the loop
        shotTimerScale(); // Calculation of weight of shot while brew is running
    }

    if (Config::getInstance().get<bool>("hardware.sensors.pressure.enabled")) {
        if (sensorManager_) {
            // Pressure reading is handled by sensorManager->update() call in ProcessController
            g_state.sensors.inputPressure = sensorManager_->getCurrentPressure();
            g_state.sensors.inputPressureFilter = sensorManager_->getFilteredPressure();
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
}

void LoopManager::updateSwitchesAndStandby() {
    // Switch handling and standby management extracted from loopPid()
    extern void checkSteamSwitch();
    extern void checkPowerSwitch();
    extern void updateStandbyTimer();

    checkSteamSwitch();
    checkPowerSwitch();
    updateStandbyTimer();
}

void LoopManager::updateStateMachine() {
    // State machine updates extracted from loopPid()
    extern std::unique_ptr<StateMachine> stateMachine;
    extern void handleMachineState();
    extern void printMachineState();
    extern LegacyMachineState lastmachinestate;
    extern void hotWaterHandler();

    // Update state machine (replaces handleMachineState())
    if (stateMachine && stateMachine->isInitialized()) {
        stateMachine->update();

        // Update compatibility variables for existing code
        const int newStateId = stateMachine->getCurrentStateId();
        if (newStateId != machineState) {
            lastmachinestate = static_cast<LegacyMachineState>(machineState);
            machineState = static_cast<LegacyMachineState>(newStateId);
            printMachineState();
        }
    }
    else {
        // Fallback to old state machine if new one isn't ready
        handleMachineState();
    }

    hotWaterHandler();
    // TODO: valveSafetyShutdownCheck() - requires brewHandler.h dependencies

    // Update brew timer display state using UIManager if available
    if (Config::getInstance().get<bool>("hardware.switches.brew.enabled")) {
        if (uiManager_) {
            uiManager_->shouldDisplayBrewTimer();
        }
        else {
            extern bool shouldDisplayBrewTimer();
            shouldDisplayBrewTimer();
        }
    }
}
