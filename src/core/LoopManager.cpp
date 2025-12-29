/**
 * @file LoopManager.cpp
 * @brief Implementation of LoopManager for main loop coordination
 */

#include "clevercoffee/core/LoopManager.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/control/ProcessController.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include "clevercoffee/handlers/BrewHandler.h"
#include "clevercoffee/handlers/HotWaterHandler.h"
#include "clevercoffee/handlers/PowerHandler.h"
#include "clevercoffee/handlers/SteamHandler.h"
#include "clevercoffee/hardware/HardwareManager.h"
#include "clevercoffee/hardware/LED.h"
#include "clevercoffee/hardware/Relay.h"
#include "clevercoffee/network/MQTTManager.h"
#include "clevercoffee/network/WebServerManager.h"
#include "clevercoffee/standby.h"
#include "clevercoffee/state/StateMachine.h"
#include "clevercoffee/ui/UIManager.h"
#include "clevercoffee/utils/ModernTimer.h"

#include <Arduino.h>
#include <ArduinoOTA.h>
#include <WiFi.h>
#include <cmath>

// Forward declaration for display template function
namespace DisplayTemplateManager {
extern void printScreen();
}

// External function declarations
// checkBrewActive removed - now accessed via SystemContext->brewHandler()
extern void sendHASSIODiscoveryMsg();
extern int  getSignalStrength();
extern void disableTimer1();
extern void enableTimer1();

// WebSocket functions are now available via WebSocketEvents.h
// No stubs needed - real functionality restored

LoopManager::LoopManager(ProcessController*               processController,
                         UIManager*                       uiManager,
                         HotWaterHandler*                 hotWaterHandler,
                         CleverCoffee::SensorCoordinator* sensorCoordinator,
                         CleverCoffee::HardwareManager*   hardwareManager,
                         CleverCoffee::SystemContext*     systemContext)
    : processController_(processController), uiManager_(uiManager),
      hotWaterHandler_(hotWaterHandler), sensorCoordinator_(sensorCoordinator), 
      hardwareManager_(hardwareManager), systemContext_(systemContext), initialized_(false), 
      sensorsTimersInitialized_(false), performanceMonitoringEnabled_(false), lastLoopTime_(0), 
      maxLoopTime_(0), loopCount_(0), temperatureUpdateCount_(0), pressureUpdateCount_(0), 
      scaleUpdateCount_(0), lastTimerLogTime_(0) {
    LOG(INFO, "LoopManager created - will initialize centralized sensor timers");
}

bool LoopManager::initialize() {
    LOG(INFO, "Initializing LoopManager");

    // Setup all timers in one place
    if (!setupAllTimers()) {
        LOG(ERROR, "LoopManager: Timer setup failed");
        return false;
    }

    // Enable performance monitoring for debugging
    performanceMonitoringEnabled_ = true;

    // Initialize timing variables
    lastLoopTime_ = millis();
    maxLoopTime_  = 0;
    loopCount_    = 0;

    initialized_ = true;

    LOG(INFO, "LoopManager initialized successfully with centralized sensor timing");
    return true;
}

void LoopManager::update() {
    if (!initialized_) {
        LOG(WARNING, "LoopManager::update() called but not initialized");
        return;
    }

    // Phase 5: Update SensorCoordinator (async sensor polling)
    if (sensorCoordinator_) {
        sensorCoordinator_->update();
    }

    // Performance timing start
    unsigned long loopStartTime = millis();
    static unsigned long slowLoopCount = 0;
    static unsigned long lastSlowLoopReport = 0;

    // 1. Accept potential connections for remote logging
    unsigned long stepStart = millis();
    Logger::update();
    unsigned long loggerTime = millis() - stepStart;

    // 2. Update water tank sensor (timer-based, 200ms intervals)
    stepStart = millis();
    updateWaterTank();
    unsigned long waterTankTime = millis() - stepStart;

    // 3. Update PID settings & machine state
    stepStart = millis();
    updateProcessControl();
    unsigned long processControlTime = millis() - stepStart;

    // 4. Update LED output based on machine state
    stepStart = millis();
    updateLEDs();
    unsigned long ledTime = millis() - stepStart;

    // 5. Update network (WiFi/MQTT/OTA)
    stepStart = millis();
    updateNetwork();
    unsigned long networkTime = millis() - stepStart;

    // 6. Update website and data transmission
    stepStart = millis();
    updateWebsite();
    unsigned long websiteTime = millis() - stepStart;

    // 7. Update sensors via centralized timers
    stepStart = millis();
    updateCentralizedSensorTimers();
    unsigned long sensorsTime = millis() - stepStart;

    // 8. Update switches and standby management
    stepStart = millis();
    updateSwitchesAndStandby();
    unsigned long switchesTime = millis() - stepStart;

    // 9. Update state machine
    stepStart = millis();
    updateStateMachine();
    unsigned long stateMachineTime = millis() - stepStart;

    // 10. Update display (critical for screen refresh)
    stepStart = millis();
    updateDisplay();
    unsigned long displayTime = millis() - stepStart;

    // 11. Print timing related data to check what is causing stutters
    stepStart = millis();
    updateDebugTiming();
    unsigned long debugTime = millis() - stepStart;

    // Performance timing end
    unsigned long loopDuration = millis() - loopStartTime;

    if (performanceMonitoringEnabled_) {
        if (loopDuration > maxLoopTime_) {
            maxLoopTime_ = loopDuration;
        }
        loopCount_++;

        // Track slow loops (>100ms)
        if (loopDuration > 100) {
            slowLoopCount++;

            // Log detailed breakdown for slow loops
            LOGF(WARNING, "Slow loop detected (%lums): Logger=%lu, WaterTank=%lu, ProcessControl=%lu, LED=%lu, Network=%lu, Website=%lu, Sensors=%lu, Switches=%lu, StateMachine=%lu, Display=%lu, Debug=%lu",
                 loopDuration, loggerTime, waterTankTime, processControlTime, ledTime,
                 networkTime, websiteTime, sensorsTime, switchesTime, stateMachineTime, displayTime, debugTime);
        }

        // Report slow loop statistics every 30 seconds
        if (millis() - lastSlowLoopReport > 30000) {
            if (slowLoopCount > 0) {
                LOGF(INFO, "Loop performance: %lu slow loops (>100ms) out of %lu total loops in last 30s", slowLoopCount, loopCount_);
                slowLoopCount = 0;
            }
            lastSlowLoopReport = millis();
        }

        // Log performance statistics every 1000 loops
        if (loopCount_ % 1000 == 0) {
            LOGF(DEBUG, "LoopManager: Processed %lu loops, max duration: %lums", loopCount_, maxLoopTime_);
        }
    }
}

void LoopManager::updateLEDs() {
    // Simple LED coordination - delegate details to dedicated LED controller when available
    const auto machineState = g_state.machine.machineState;
    const auto temperature = g_state.process.temperature;
    const auto setpoint = g_state.process.setpoint;

    if (!hardwareManager_) {
        return;  // No hardware manager available
    }

    // Status LED - indicates when temperature is reached
    if (Config::getInstance().hardwareLedsStatusEnabled.get() && hardwareManager_->getStatusLed()) {
        bool shouldTurnOn = (machineState == MachineStateId::PID_NORMAL &&
                           (fabs(temperature - setpoint) < 0.3)) ||
                          (temperature > 115 && fabs(temperature - setpoint) < 5);

        shouldTurnOn ? hardwareManager_->getStatusLed()->turnOn() : hardwareManager_->getStatusLed()->turnOff();
    }

    // Brew LED - indicates brewing state
    if (Config::getInstance().hardwareLedsBrewEnabled.get() && hardwareManager_->getBrewLed()) {
        isBrewState(machineState) ? hardwareManager_->getBrewLed()->turnOn() : hardwareManager_->getBrewLed()->turnOff();
    }

    // Steam LED - indicates steam mode
    if (Config::getInstance().hardwareLedsSteamEnabled.get() && hardwareManager_->getSteamLed()) {
        isSteamState(machineState) ? hardwareManager_->getSteamLed()->turnOn() : hardwareManager_->getSteamLed()->turnOff();
    }
}

void LoopManager::updateWaterTank() {
    // Water tank monitoring is handled by the timer-based system
    if (waterTankTimer_) {
        // Advance the timer so checkWaterTank() is called at the correct interval
        (*waterTankTimer_)();
    } else {
        // Fallback: direct call to water tank check
        // This should normally not be needed if timer is set up correctly
        // checkWaterTank();
        LOG(WARNING, "LoopManager: Water tank timer not initialized. Can not update water tank status.");
    }
}

void LoopManager::updateProcessControl() {
    if (processController_) {
        // Use modern ProcessController for PID and temperature management
        const unsigned long processStart = millis();
        processController_->updateProcessControl(g_state.machine.machineState);
        const unsigned long processTime = millis() - processStart;

        if (processTime > 100) {
            LOGF(ERROR, "ProcessController update took %lums - this is blocking the main loop!", processTime);
        } else if (processTime > 50) {
            LOGF(WARNING, "ProcessController update took %lums", processTime);
        }
    } else {
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
            bool websiteCondition = systemContext_ ? !systemContext_->uiCoordinator().isWebsiteUpdateRunning() : true;
            bool mqttCondition    = (!g_state.network.mqttManager || !g_state.network.mqttManager->isUpdateRunning());
            bool hassioCondition  = systemContext_ ? !systemContext_->uiCoordinator().isHassioUpdateRunning() : true;
            bool tempCondition    = systemContext_ ? !systemContext_->sensorCoordinator().isTemperatureUpdateRunning() : true;
            bool standbyCondition = systemContext_ ?
                (!Config::getInstance().standbyEnabled.get() || systemContext_->standbyCoordinator().getRemainingTimeMillis() > 0) :
                (!Config::getInstance().standbyEnabled.get() || g_state.standby.standbyModeRemainingTimeMillis > 0);

            // update display on loops that have not had other major tasks running
            if (websiteCondition && mqttCondition && hassioCondition && tempCondition && standbyCondition) {
                if (uiManager_->isBufferReady()) {
                    uiManager_->forceUpdate();
                    uiManager_->setBufferReady(false);
                    uiManager_->setUpdateRunning(true);
                } else {
                    // This is the critical call that was missing!
                    // It triggers the display template rendering
                    if (printDisplayTimer_) {
                        LOGF(DEBUG, "LoopManager: Calling printDisplayTimer (UIManager path)");
                        (*printDisplayTimer_)();
                    } else {
                        LOGF(WARNING, "LoopManager: printDisplayTimer is null!");
                    }
                }
            }
        }
    } else {
        LOGF(WARNING, "LoopManager: UIManager not available, skipping display update");
    }
}

void LoopManager::updateDebugTiming() {
    // Debug timing function removed - was unused
    // This function is kept for API compatibility but does nothing
}

bool LoopManager::setupAllTimers() {
    try {
        LOG(INFO, "LoopManager: Setting up all timers");

        // 1. General timers (display, HASSIO discovery)
        hassioDiscoveryTimer_ =
            std::make_unique<MillisecondTimer>(&sendHASSIODiscoveryMsg, std::chrono::milliseconds(300000));
        printDisplayTimer_ =
            std::make_unique<MillisecondTimer>(DisplayTemplateManager::printScreen, std::chrono::milliseconds(100));
        LOG(INFO, "LoopManager: General timers initialized");

        // 2. Water tank monitoring timer (200ms interval)
        waterTankTimer_ = std::make_unique<MillisecondTimer>(
            std::bind(&LoopManager::checkWaterTankLevel, this),
            std::chrono::milliseconds(200));
        if (!waterTankTimer_) {
            LOG(ERROR, "LoopManager: Failed to create water tank timer");
            return false;
        }
        LOG(INFO, "LoopManager: Water tank timer initialized (200ms interval)");

        // 3. Centralized sensor timers
        // Temperature sensor timer (400ms - 2.5Hz for PID stability, Dallas DS18B20 takes ~100ms to read)
        temperatureTimer_ = std::make_unique<MillisecondTimer>(
            std::bind(&LoopManager::updateTemperatureSensor, this),
            std::chrono::milliseconds(400));

        // Pressure sensor timer (50ms - 20Hz for responsiveness)
        pressureTimer_ = std::make_unique<MillisecondTimer>(
            std::bind(&LoopManager::updatePressureSensor, this),
            std::chrono::milliseconds(50));

        // Scale sensor timer (100ms - 10Hz for good balance)
        scaleTimer_ = std::make_unique<MillisecondTimer>(
            std::bind(&LoopManager::updateScaleSensor, this),
            std::chrono::milliseconds(100));

        sensorsTimersInitialized_ = true;
        LOG(INFO, "LoopManager: Centralized sensor timers initialized");

        LOG(INFO, "LoopManager: All timers successfully initialized");
        return true;
    } catch (const std::exception& e) {
        LOGF(ERROR, "LoopManager: Exception creating timers: %s", e.what());
        return false;
    }
}

void LoopManager::checkWaterTankLevel() {
    if (sensorCoordinator_) {
        // SensorCoordinator auto-updates water tank sensor
        g_state.machine.waterTankFull = sensorCoordinator_->isWaterTankFull();
    }
}

void LoopManager::updateTemperatureSensor() {
    // SensorCoordinator auto-updates temperature sensor - no manual update needed
    // Temperature updates are handled by SensorCoordinator.update() called in the main loop
    temperatureUpdateCount_++;
}

void LoopManager::updatePressureSensor() {
    if (Config::getInstance().hardwareSensorsPressureEnabled.get() && sensorCoordinator_) {
        // SensorCoordinator auto-updates pressure sensor
        g_state.sensors.inputPressure = sensorCoordinator_->getPressure();
        g_state.sensors.inputPressureFilter = sensorCoordinator_->getFilteredPressure();

        pressureUpdateCount_++;
    }
}

void LoopManager::updateScaleSensor() {
    if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        const unsigned long startTime = millis();

        // Update current reading weight from SensorCoordinator
        g_state.sensors.currReadingWeight = static_cast<float>(sensorCoordinator_->getWeight());

        // Calculate brew weight (current weight - pre-brew weight)
        updateBrewWeight();

        const unsigned long updateTime = millis() - startTime;
        if (updateTime > 50) {
            LOGF(WARNING, "Scale sensor update took %lums", updateTime);
        }

        scaleUpdateCount_++;
    }
}

void LoopManager::updateBrewWeight() {
    if (!sensorCoordinator_) {
        return;  // No sensor coordinator available
    }
    
    // Simple state machine: start tracking when brew starts, stop when brew ends
    const auto currentState = g_state.machine.machineState;
    const bool isBrewActive = (currentState != MachineStateId::BREW_IDLE);
    
    // Check if we need to start brew weight tracking
    if (isBrewActive && !sensorCoordinator_->isBrewWeightTrackingActive()) {
        sensorCoordinator_->startBrewWeightTracking();
    }
    // Check if we need to stop brew weight tracking
    else if (!isBrewActive && sensorCoordinator_->isBrewWeightTrackingActive()) {
        sensorCoordinator_->stopBrewWeightTracking();
    }
    
    // Update global state for backward compatibility (will be removed in Phase 10.4)
    g_state.sensors.currBrewWeight = static_cast<float>(sensorCoordinator_->getBrewWeight());
    g_state.sensors.preBrewWeight = static_cast<float>(sensorCoordinator_->getPreBrewWeight());
}

void LoopManager::updateCentralizedSensorTimers() {
    if (sensorsTimersInitialized_) {
        // Invoke sensor timers - they handle their own timing intervals
        if (temperatureTimer_) (*temperatureTimer_)();
        if (pressureTimer_) (*pressureTimer_)();
        if (scaleTimer_) (*scaleTimer_)();

        // Performance monitoring every 30 seconds
        const unsigned long currentTime = millis();
        if (currentTime - lastTimerLogTime_ >= 30000) {
            logTimerConfiguration();
            lastTimerLogTime_ = currentTime;
        }
    }
}

bool LoopManager::getPerformanceStats() const {
    return performanceMonitoringEnabled_ && loopCount_ > 0;
}

void LoopManager::updateNetwork() {
    // Simplified network coordination - delegate complex logic to network managers
    static bool wifiWasConnected = false;

    // Check offline mode from NetworkCoordinator
    bool isOfflineMode = (systemContext_ && systemContext_->networkCoordinator().isOfflineMode()) || 
                         (!systemContext_ && g_state.network.offlineMode);

    if (WiFi.status() == WL_CONNECTED && !isOfflineMode) {
        if (!wifiWasConnected) {
            LOG(INFO, "WiFi Connected");
            wifiWasConnected = true;
        }

        // MQTT Management - delegate to MQTTManager
        if (g_state.network.mqttManager && g_state.network.mqttManager->isEnabled()) {
            g_state.network.mqttManager->setUpdateRunning(false);

            if (g_state.network.cleverCoffeeWiFiManager->getSignalStrength() > 1) {
                g_state.network.mqttManager->checkConnection();

                bool displayBufferNotReady = systemContext_ ? !systemContext_->uiCoordinator().isDisplayBufferReady() : true;
                bool tempNotRunning = systemContext_ ? !systemContext_->sensorCoordinator().isTemperatureUpdateRunning() : true;
                if (displayBufferNotReady && tempNotRunning) {
                    g_state.network.mqttManager->writeSysParamsToMQTT(true);
                }
            }

            if (g_state.network.mqttManager->isConnected()) {
                g_state.network.mqttManager->loop();
                // Home Assistant discovery - delegate to timer system
                bool displayBufferNotReady = systemContext_ ? !systemContext_->uiCoordinator().isDisplayBufferReady() : true;
                bool tempNotRunning = systemContext_ ? !systemContext_->sensorCoordinator().isTemperatureUpdateRunning() : true;
                if (hassioDiscoveryTimer_ && displayBufferNotReady && tempNotRunning) {
                    (*hassioDiscoveryTimer_)();
                }
                g_state.network.mqttManager->setWasConnected(true);
            } else if (g_state.network.mqttManager->wasConnected()) {
                LOG(INFO, "MQTT disconnected");
                g_state.network.mqttManager->setWasConnected(false);
            }
        }

        // OTA handling - minimal coordination
        ArduinoOTA.handle();
        
        // Reset WiFi reconnection counter on successful connection
        if (systemContext_) {
            systemContext_->networkCoordinator().resetWifiReconnects();
        } else {
            g_state.network.wifiReconnects = 0;
        }
     } else {
         wifiWasConnected = false;
         if (g_state.network.cleverCoffeeWiFiManager) {
             g_state.network.cleverCoffeeWiFiManager->checkAndMaintainConnection();
         }
     }

    // Backward compatibility sync: Copy NetworkCoordinator state back to g_state
    // This ensures display code and other components still work during transition
    if (systemContext_) {
        g_state.network.offlineMode = systemContext_->networkCoordinator().isOfflineMode();
        g_state.network.wifiReconnects = systemContext_->networkCoordinator().getWifiReconnects();
        g_state.network.lastWifiConnectionAttempt = systemContext_->networkCoordinator().getLastWifiConnectionAttempt();
        g_state.network.lastMQTTConnectionAttempt = systemContext_->networkCoordinator().getLastMqttConnectionAttempt();
        
         // Backward compatibility sync: Copy coordinator flags back to g_state.coordination
         g_state.coordination.temperatureUpdateRunning = systemContext_->sensorCoordinator().isTemperatureUpdateRunning();
         g_state.coordination.displayBufferReady = systemContext_->uiCoordinator().isDisplayBufferReady();
         g_state.coordination.websiteUpdateRunning = systemContext_->uiCoordinator().isWebsiteUpdateRunning();
         g_state.coordination.hassioUpdateRunning = systemContext_->uiCoordinator().isHassioUpdateRunning();
         
         // Backward compatibility sync: Copy sensor scale modes back to g_state.sensors
         g_state.sensors.scaleTareOn = systemContext_->sensorCoordinator().isScaleTareMode();
         g_state.sensors.scaleCalibrationOn = systemContext_->sensorCoordinator().isScaleCalibrationMode();
         
         // Backward compatibility sync: Copy display state back to g_state.display
        g_state.display.displayOffline = systemContext_->uiCoordinator().getDisplayOffline();
        
        // Backward compatibility sync: Copy standby state back to g_state.standby
        g_state.standby.standbyModeRemainingTimeMillis = systemContext_->standbyCoordinator().getRemainingTimeMillis();

        // Backward compatibility sync: Copy process state back to g_state.process
        if (systemContext_->processController()) {
            g_state.process.temperature = systemContext_->processController()->getCurrentTemperature();
            g_state.process.pidOutput = systemContext_->processController()->getPIDOutput();
            g_state.process.setpoint = systemContext_->processController()->getSetpoint();
            g_state.process.currBrewTime = systemContext_->processController()->getCurrBrewTime();
            g_state.process.totalTargetBrewTime = systemContext_->processController()->getTotalTargetBrewTime();
            g_state.process.brewPidDisabled = systemContext_->processController()->isBrewPidDisabled();
        }
    }
}

void LoopManager::updateWebsite() {
    // Simplified website coordination - delegate to WebServerManager
    bool hassioNotRunning = systemContext_ ? !systemContext_->uiCoordinator().isHassioUpdateRunning() : true;
    bool displayBufferNotReady = systemContext_ ? !systemContext_->uiCoordinator().isDisplayBufferReady() : true;
    bool tempNotRunning = systemContext_ ? !systemContext_->sensorCoordinator().isTemperatureUpdateRunning() : true;
    
    const bool canSendData = (millis() - lastTempEvent_) > tempEventInterval_ &&
                            (!g_state.network.mqttManager || !g_state.network.mqttManager->isUpdateRunning()) &&
                            hassioNotRunning &&
                            displayBufferNotReady &&
                            tempNotRunning;

    // Check offline mode from NetworkCoordinator
    bool isOfflineMode = (systemContext_ && systemContext_->networkCoordinator().isOfflineMode()) || 
                         (!systemContext_ && g_state.network.offlineMode);

    if (canSendData && WiFi.status() == WL_CONNECTED && !isOfflineMode) {
        if (systemContext_) {
            systemContext_->uiCoordinator().setWebsiteUpdateRunning(true);
        }

        // Delegate to WebServerManager for actual transmission
        if (g_state.network.webServerManager) {
            g_state.network.webServerManager->sendTempEvent(
                g_state.process.temperature,
                Config::getInstance().brewSetpoint.get(),
                g_state.process.pidOutput / 10); // Convert promill to percent

            if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
                g_state.network.webServerManager->sendWeightEvent();
            }
        }

        lastTempEvent_ = millis();
        if (systemContext_) {
            systemContext_->uiCoordinator().setWebsiteUpdateRunning(false);
        }
    }
}

// Old updateSensors() method removed - replaced with centralized timer system

void LoopManager::updateSwitchesAndStandby() {
    // Switch handling and standby management extracted from loopPid()
    if (systemContext_) {
        if (auto* steamHandler = systemContext_->steamHandler()) {
            steamHandler->process();
        }
        if (auto* powerHandler = systemContext_->powerHandler()) {
            powerHandler->process();
        }
    }
    
    // Update standby timer through coordinator
    if (systemContext_) {
        systemContext_->standbyCoordinator().update();
    } else {
        updateStandbyTimer(); // Fallback to legacy function
    }
}

void LoopManager::updateStateMachine() {
    // State machine updates extracted from loopPid()
    extern std::unique_ptr<StateMachine> stateMachine;
    // Update state machine (replaces handleMachineState())
    if (stateMachine && stateMachine->isInitialized()) {
        stateMachine->update();

        // Update machine state
        const MachineStateId newState = stateMachine->getCurrentStateId();
        if (newState != g_state.machine.machineState) {
            const auto oldState              = g_state.machine.machineState;
            g_state.machine.lastmachinestate = g_state.machine.machineState;
            g_state.machine.machineState     = newState;
            LOGF(DEBUG, "State transition: %d -> %d", static_cast<int>(oldState), static_cast<int>(newState));
        }
    } else {
        // StateMachine should always be available in modern setup
        LOG(WARNING, "StateMachine not available for state updates");
    }

    // Update handlers
    if (systemContext_) {
        if (auto* hotWaterHandler = systemContext_->hotWaterHandler()) {
            hotWaterHandler->process();
        }
        if (auto* brewHandler = systemContext_->brewHandler()) {
            brewHandler->process();
            brewHandler->valveSafetyShutdownCheck();
        }
    }

    // Update brew timer display state using UIManager if available
    if (Config::getInstance().hardwareSwitchesBrewEnabled.get()) {
        if (uiManager_) {
            uiManager_->shouldDisplayBrewTimer();
        } else {
            extern bool shouldDisplayBrewTimer();
            shouldDisplayBrewTimer();
        }
    }
}

void LoopManager::configureSensorTimers(unsigned long temperatureIntervalMs,
                                        unsigned long pressureIntervalMs,
                                        unsigned long scaleIntervalMs) {
    try {
        // Recreate timer objects with new intervals
        temperatureTimer_ = std::make_unique<MillisecondTimer>(
            std::bind(&LoopManager::updateTemperatureSensor, this),
            std::chrono::milliseconds(temperatureIntervalMs));

        pressureTimer_ = std::make_unique<MillisecondTimer>(
            std::bind(&LoopManager::updatePressureSensor, this),
            std::chrono::milliseconds(pressureIntervalMs));

        scaleTimer_ = std::make_unique<MillisecondTimer>(
            std::bind(&LoopManager::updateScaleSensor, this),
            std::chrono::milliseconds(scaleIntervalMs));

        // Reset counters when reconfiguring
        temperatureUpdateCount_ = 0;
        pressureUpdateCount_ = 0;
        scaleUpdateCount_ = 0;
        lastTimerLogTime_ = millis();

        LOGF(INFO, "Sensor timers reconfigured - Temperature: %lums (%.1fHz), Pressure: %lums (%.1fHz), Scale: %lums (%.1fHz)",
             temperatureIntervalMs, 1000.0f / temperatureIntervalMs,
             pressureIntervalMs, 1000.0f / pressureIntervalMs,
             scaleIntervalMs, 1000.0f / scaleIntervalMs);
    } catch (const std::exception& e) {
        LOGF(ERROR, "LoopManager: Exception reconfiguring sensor timers: %s", e.what());
    }
}

void LoopManager::logTimerConfiguration() const {
    const unsigned long currentTime = millis();
    const float timeDiffSeconds = (currentTime - lastTimerLogTime_) / 1000.0f;

    if (timeDiffSeconds > 0) {
        const float actualTempFreq = temperatureUpdateCount_ / timeDiffSeconds;
        const float actualPressureFreq = pressureUpdateCount_ / timeDiffSeconds;
        const float actualScaleFreq = scaleUpdateCount_ / timeDiffSeconds;

        LOGF(INFO, "Centralized Sensor Timer Performance Report:");
        LOGF(INFO, "  Temperature: %.1fHz actual (%lu updates)", actualTempFreq, temperatureUpdateCount_);
        LOGF(INFO, "  Pressure: %.1fHz actual (%lu updates)", actualPressureFreq, pressureUpdateCount_);
        LOGF(INFO, "  Scale: %.1fHz actual (%lu updates)", actualScaleFreq, scaleUpdateCount_);
    }

    // Reset counters for next measurement period
    const_cast<LoopManager*>(this)->temperatureUpdateCount_ = 0;
    const_cast<LoopManager*>(this)->pressureUpdateCount_ = 0;
    const_cast<LoopManager*>(this)->scaleUpdateCount_ = 0;
}
