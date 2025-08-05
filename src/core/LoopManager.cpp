/**
 * @file LoopManager.cpp
 * @brief Implementation of LoopManager for main loop coordination
 */

#include "LoopManager.h"
#include "../Config.h"
#include "../brewHandler.h"
#include "../control/ProcessController.h"
#include "../hardware/LED.h"
#include "../hardware/Relay.h"
#include "../hotWaterHandler.h"
#include "../network/MQTTManager.h"
#include "../network/WebServerManager.h"
#include "../powerHandler.h"
#include "../sensors/SensorManager.h"
#include "../standby.h"
#include "../state/GlobalState.h"
#include "../state/StateMachine.h"
#include "../steamHandler.h"
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

// External function declarations
extern bool checkBrewActive();
extern void checkWaterTank();
extern void sendHASSIODiscoveryMsg();
extern void checkWifi();
extern int getSignalStrength();
extern void disableTimer1();
extern void enableTimer1();

// WebSocket functions are now available via WebSocketEvents.h
// No stubs needed - real functionality restored

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
    if (Config::getInstance().hardwareLedsStatusEnabled.get() && g_state.hardware.statusLed != nullptr) {
        bool shouldTurnOn = false;

        // Turn on when at target temperature (normal or steam mode)
        if ((g_state.machine.machineState == kPidNormal && (fabs(g_state.process.temperature - g_state.process.setpoint) < 0.3)) ||
            (g_state.process.temperature > 115 && fabs(g_state.process.temperature - g_state.process.setpoint) < 5)) {
            shouldTurnOn = true;
        }

        if (shouldTurnOn) {
            g_state.hardware.statusLed->turnOn();
        }
        else {
            g_state.hardware.statusLed->turnOff();
        }
    }

    // Brew LED - indicates brewing state
    if (Config::getInstance().hardwareLedsBrewEnabled.get() && g_state.hardware.brewLed != nullptr) {
        if (g_state.machine.machineState == kBrew) {
            g_state.hardware.brewLed->turnOn();
        }
        else {
            g_state.hardware.brewLed->turnOff();
        }
    }

    // Steam LED - indicates steam mode
    if (Config::getInstance().hardwareLedsSteamEnabled.get() && g_state.hardware.steamLed != nullptr) {
        if (g_state.machine.machineState == kSteam) {
            g_state.hardware.steamLed->turnOn();
        }
        else {
            g_state.hardware.steamLed->turnOff();
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
        processController_->updateProcessControl(g_state.machine.machineState);
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

        if (Config::getInstance().hardwareOledEnabled.get()) {
            // Only update display on loops that have not had other major tasks running
            // and when not in standby display-off mode
            bool websiteCondition = !g_state.coordination.websiteUpdateRunning;
            bool mqttCondition = (!g_state.network.mqttManager || !g_state.network.mqttManager->isUpdateRunning());
            bool hassioCondition = !g_state.coordination.hassioUpdateRunning;
            bool tempCondition = !g_state.coordination.temperatureUpdateRunning;
            bool standbyCondition = (!Config::getInstance().standbyEnabled.get() || g_state.standby.standbyModeRemainingTimeMillis > 0);

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

        if (Config::getInstance().hardwareOledEnabled.get()) {
            // Only update display on loops that have not had other major tasks running
            // and when not in standby display-off mode
            bool websiteCondition = !g_state.coordination.websiteUpdateRunning;
            bool mqttCondition = (!g_state.network.mqttManager || !g_state.network.mqttManager->isUpdateRunning());
            bool hassioCondition = !g_state.coordination.hassioUpdateRunning;
            bool tempCondition = !g_state.coordination.temperatureUpdateRunning;
            bool standbyCondition = (!Config::getInstance().standbyEnabled.get() || g_state.standby.standbyModeRemainingTimeMillis > 0);

            // update display on loops that have not had other major tasks running
            if (websiteCondition && mqttCondition && hassioCondition && tempCondition && standbyCondition) {
                if (g_state.coordination.displayBufferReady) {
                    g_state.hardware.display->sendBuffer();
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

    // Only do Wifi stuff, if Wifi is connected
    if (WiFi.status() == WL_CONNECTED && !g_state.network.offlineMode) {
        if (wifiWasConnected == false) {
            LOG(INFO, "WiFi Connected");
            wifiWasConnected = true;
        }

        if (g_state.network.mqttManager && g_state.network.mqttManager->isEnabled()) {
            g_state.network.mqttManager->setUpdateRunning(false);

            if (g_state.network.cleverCoffeeWiFiManager->getSignalStrength() > 1) {
                g_state.network.mqttManager->checkConnection();

                // if screen is ready to refresh wait for next loop
                if (!g_state.coordination.displayBufferReady && !g_state.coordination.temperatureUpdateRunning) {
                    g_state.network.mqttManager->writeSysParamsToMQTT(true);
                }
            }

            g_state.coordination.hassioUpdateRunning = false;

            if (g_state.network.mqttManager->isConnected()) {
                g_state.network.mqttManager->loop();

                // resend discovery messages if not during a main function and MQTT has been disconnected but has now reconnected
                if (!(g_state.machine.machineState >= kBrew && g_state.machine.machineState <= kBackflush) &&
                    ((!g_state.network.mqttManager->wasConnected() || g_state.network.hassioFailed) && !g_state.coordination.displayBufferReady && !g_state.coordination.temperatureUpdateRunning)) {
                    if (g_state.timing.hassioDiscoveryTimer) (*g_state.timing.hassioDiscoveryTimer)();
                }

                g_state.network.mqttManager->setWasConnected(true);
            }
            else if (g_state.network.mqttManager->wasConnected()) {
                LOG(INFO, "MQTT disconnected");
                g_state.network.mqttManager->setWasConnected(false);
            }
        }

        // OTA handling
        ArduinoOTA.handle();

        // Disable interrupt if OTA is starting, otherwise it will not work
        ArduinoOTA.onStart([]() {
            disableTimer1();
            if (g_state.hardware.heaterRelay) g_state.hardware.heaterRelay->off();
        });

        ArduinoOTA.onError([](ota_error_t error) { enableTimer1(); });
        ArduinoOTA.onEnd([]() { enableTimer1(); });

        g_state.network.wifiReconnects = 0; // reset wifi reconnects if connected
    }
    else {
        wifiWasConnected = false;
        checkWifi();
    }
}

void LoopManager::updateWebsite() {
    // Website and data transmission updates extracted from loopPid()
    // pidOutput moved to g_state.process.pidOutput

    bool timeCondition = (millis() - g_state.network.lastTempEvent) > g_state.network.tempEventInterval;
    bool mqttCondition = (!g_state.network.mqttManager || !g_state.network.mqttManager->isUpdateRunning());
    bool hassioCondition = !g_state.coordination.hassioUpdateRunning;
    bool displayCondition = !g_state.coordination.displayBufferReady;
    bool tempCondition = !g_state.coordination.temperatureUpdateRunning;

    if (timeCondition && mqttCondition && hassioCondition && displayCondition && tempCondition) {
        LOGF(DEBUG, "LoopManager: Conditions met for sending temperature events");
        g_state.coordination.websiteUpdateRunning = true;

        // send temperatures to website endpoint
        if (WiFi.status() == WL_CONNECTED && !g_state.network.offlineMode) {
            LOGF(DEBUG, "LoopManager: Sending temperature event: temp=%.2f, setpoint=%.2f, output=%.2f", g_state.process.temperature, Config::getInstance().brewSetpoint.get(), g_state.process.pidOutput / 10);
            g_state.network.webServerManager->sendTempEvent(g_state.process.temperature, Config::getInstance().brewSetpoint.get(), g_state.process.pidOutput / 10); // pidOutput is promill, so /10 to get percent value

            if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
                g_state.network.webServerManager->sendWeightEvent();
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

    if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        checkWeight();    // Check Weight Scale in the loop
        shotTimerScale(); // Calculation of weight of shot while brew is running
    }

    if (Config::getInstance().hardwareSensorsPressureEnabled.get()) {
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
    checkSteamSwitch();
    checkPowerSwitch();
    updateStandbyTimer();
}

void LoopManager::updateStateMachine() {
    // State machine updates extracted from loopPid()
    extern std::unique_ptr<StateMachine> stateMachine;
    extern void printMachineState();

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
        // StateMachine should always be available in modern setup
        LOG(WARNING, "StateMachine not available for state updates");
    }

    hotWaterHandler();
    // TODO: valveSafetyShutdownCheck() - requires brewHandler.h dependencies

    // Update brew timer display state using UIManager if available
    if (Config::getInstance().hardwareSwitchesBrewEnabled.get()) {
        if (uiManager_) {
            uiManager_->shouldDisplayBrewTimer();
        }
        else {
            extern bool shouldDisplayBrewTimer();
            shouldDisplayBrewTimer();
        }
    }
}
