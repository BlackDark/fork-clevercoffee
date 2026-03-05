/**
 * @file LoopManager.cpp
 * @brief Implementation of LoopManager for main loop coordination
 */

#include "clevercoffee/core/LoopManager.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/constants/Temperature.h"
#include "clevercoffee/constants/Timing.h"
#include "clevercoffee/context/SystemContext.h"
#include "clevercoffee/control/ProcessController.h"
#include "clevercoffee/coordinators/NetworkCoordinator.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"
#include "clevercoffee/coordinators/UICoordinator.h"
#include "clevercoffee/display/displayCommon.h"
#include "clevercoffee/handlers/BrewHandler.h"
#include "clevercoffee/handlers/HotWaterHandler.h"
#include "clevercoffee/handlers/PowerHandler.h"
#include "clevercoffee/handlers/SteamHandler.h"
#include "clevercoffee/hardware/HardwareManager.h"
#include "clevercoffee/hardware/LED.h"
#include "clevercoffee/hardware/Relay.h"
#include "clevercoffee/network/MQTTManager.h"
#include "clevercoffee/network/WebServerManager.h"
#include "clevercoffee/state/StateMachine.h"
#include "clevercoffee/types/GlobalTypes.h"
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
extern int  getSignalStrength();
extern void disableTimer1();
extern void enableTimer1();

// WebSocket functions are now available via WebSocketEvents.h
// No stubs needed - real functionality restored

LoopManager::LoopManager(CleverCoffee::SystemContext&     systemContext,
                         CleverCoffee::HardwareManager&   hardwareManager,
                         ProcessController&               processController,
                         CleverCoffee::SensorCoordinator& sensorCoordinator,
                         UIManager&                       uiManager)
    : systemContext_(systemContext), hardwareManager_(hardwareManager), processController_(processController),
      sensorCoordinator_(sensorCoordinator), uiManager_(uiManager), initialized_(false),
      sensorsTimersInitialized_(false), performanceMonitoringEnabled_(false), lastLoopTime_(0), maxLoopTime_(0),
      loopCount_(0), temperatureUpdateCount_(0), pressureUpdateCount_(0), scaleUpdateCount_(0), lastTimerLogTime_(0) {
    LOG(INFO, "LoopManager created - will initialize centralized sensor timers");
}

LoopManager::~LoopManager() = default;

bool LoopManager::initialize() {
    LOG(INFO, "Initializing LoopManager");

    // Setup all timers in one place
    if (!setupAllTimers()) {
        LOG(ERROR, "LoopManager: Timer setup failed");
        return false;
    }

// Enable performance monitoring only in debug builds
#ifdef DEBUG
    performanceMonitoringEnabled_ = true;
#else
    performanceMonitoringEnabled_ = false;
#endif

    // Initialize timing variables
    lastLoopTime_ = millis();
    maxLoopTime_  = 0;
    loopCount_    = 0;

    initialized_ = true;

    LOG(INFO, "LoopManager initialized successfully");
    return true;
}

void LoopManager::update() {
    if (!initialized_) {
        LOG(WARNING, "LoopManager::update() called but not initialized");
        return;
    }

    // Cache millis() at start to avoid multiple calls - reuse throughout
    const unsigned long loopStartTime = millis();

    // Performance monitoring variables (only used when enabled)
    unsigned long        stepStart          = 0;
    unsigned long        loggerTime         = 0;
    unsigned long        hardwareTime       = 0;
    unsigned long        networkTime        = 0;
    unsigned long        websiteTime        = 0;
    unsigned long        sensorsTime        = 0;
    unsigned long        switchesTime       = 0;
    unsigned long        stateMachineTime   = 0;
    unsigned long        displayTime        = 0;
    static unsigned long slowLoopCount      = 0;
    static unsigned long lastSlowLoopReport = 0;

    // 1. Logger updates (remote logging connections)
    if (performanceMonitoringEnabled_) {
        stepStart = millis();
    }
    Logger::update();
    if (performanceMonitoringEnabled_) {
        loggerTime = millis() - stepStart;
    }

    // 2. Hardware updates (water tank, process control, LEDs)
    if (performanceMonitoringEnabled_) {
        stepStart = millis();
    }
    updateWaterTank();
    updateProcessControl();
    updateLEDs();
    if (performanceMonitoringEnabled_) {
        hardwareTime = millis() - stepStart;
    }

    // 3. Network updates (WiFi/MQTT/OTA/WebServer)
    if (performanceMonitoringEnabled_) {
        stepStart = millis();
    }
    updateNetwork();
    if (performanceMonitoringEnabled_) {
        networkTime = millis() - stepStart;
    }

    // 4. Website data transmission
    if (performanceMonitoringEnabled_) {
        stepStart = millis();
    }
    updateWebsite();
    if (performanceMonitoringEnabled_) {
        websiteTime = millis() - stepStart;
    }

    // 5. Sensor updates
    if (performanceMonitoringEnabled_) {
        stepStart = millis();
    }
    sensorCoordinator_.update();
    updateCentralizedSensorTimers();
    if (performanceMonitoringEnabled_) {
        sensorsTime = millis() - stepStart;
    }

    // 6. Switches and standby management
    if (performanceMonitoringEnabled_) {
        stepStart = millis();
    }
    updateSwitchesAndStandby();
    if (performanceMonitoringEnabled_) {
        switchesTime = millis() - stepStart;
    }

    // 7. State machine updates
    if (performanceMonitoringEnabled_) {
        stepStart = millis();
    }
    updateStateMachine();
    if (performanceMonitoringEnabled_) {
        stateMachineTime = millis() - stepStart;
    }

    // 8. Display updates
    if (performanceMonitoringEnabled_) {
        stepStart = millis();
    }
    // Handle display timer callback (managed by LoopManager) - call timer first to generate content
    // Timer should fire periodically regardless of buffer state to generate new display content
    if (printDisplayTimer_) {
        (*printDisplayTimer_)();
    }
    // Then update display (send buffer if ready)
    updateDisplay();
    if (performanceMonitoringEnabled_) {
        displayTime = millis() - stepStart;
    }

    // Performance monitoring (only when enabled)
    if (performanceMonitoringEnabled_) {
        const unsigned long loopDuration = millis() - loopStartTime;

        if (loopDuration > maxLoopTime_) {
            maxLoopTime_ = loopDuration;
        }
        loopCount_++;

        // Track slow loops
        using CleverCoffee::Timing::SLOW_LOOP_THRESHOLD_MS;
        if (loopDuration > SLOW_LOOP_THRESHOLD_MS) {
            slowLoopCount++;
            LOGF(WARNING,
                 "Slow loop detected (%lums): Logger=%lu, Hardware=%lu, Network=%lu, Website=%lu, Sensors=%lu, "
                 "Switches=%lu, StateMachine=%lu, Display=%lu",
                 loopDuration,
                 loggerTime,
                 hardwareTime,
                 networkTime,
                 websiteTime,
                 sensorsTime,
                 switchesTime,
                 stateMachineTime,
                 displayTime);
        }

        // Report slow loop statistics periodically
        using CleverCoffee::Timing::SLOW_LOOP_REPORT_INTERVAL_MS;
        const unsigned long now = millis();
        if (now - lastSlowLoopReport > SLOW_LOOP_REPORT_INTERVAL_MS) {
            if (slowLoopCount > 0) {
                LOGF(INFO,
                     "Loop performance: %lu slow loops (>100ms) out of %lu total loops in last 30s",
                     slowLoopCount,
                     loopCount_);
                slowLoopCount = 0;
            }
            lastSlowLoopReport = now;
        }

        // Log performance statistics periodically
        using CleverCoffee::Timing::PERFORMANCE_LOG_INTERVAL_LOOPS;
        if (loopCount_ % PERFORMANCE_LOG_INTERVAL_LOOPS == 0) {
            LOGF(DEBUG, "LoopManager: Processed %lu loops, max duration: %lums", loopCount_, maxLoopTime_);
        }
    }
}

void LoopManager::updateLEDs() {
    const auto machineState = systemContext_.machineStateContext()->getCurrentStateId();
    const auto temperature  = systemContext_.processTemperature();
    const auto setpoint     = systemContext_.processSetpoint();

    using CleverCoffee::Temperature::STEAM_LED_THRESHOLD_C;
    using CleverCoffee::Temperature::TEMP_TOLERANCE_NORMAL_C;
    using CleverCoffee::Temperature::TEMP_TOLERANCE_STEAM_C;

    if (Config::getInstance().hardwareLedsStatusEnabled.get() && hardwareManager_.getStatusLed()) {
        bool shouldTurnOn =
            (machineState == MachineStateId::PID_NORMAL && (fabs(temperature - setpoint) < TEMP_TOLERANCE_NORMAL_C)) ||
            (temperature > STEAM_LED_THRESHOLD_C && fabs(temperature - setpoint) < TEMP_TOLERANCE_STEAM_C);
        shouldTurnOn ? hardwareManager_.getStatusLed()->turnOn() : hardwareManager_.getStatusLed()->turnOff();
    }

    if (Config::getInstance().hardwareLedsBrewEnabled.get() && hardwareManager_.getBrewLed()) {
        isBrewState(machineState) ? hardwareManager_.getBrewLed()->turnOn() : hardwareManager_.getBrewLed()->turnOff();
    }

    if (Config::getInstance().hardwareLedsSteamEnabled.get() && hardwareManager_.getSteamLed()) {
        isSteamState(machineState) ? hardwareManager_.getSteamLed()->turnOn()
                                   : hardwareManager_.getSteamLed()->turnOff();
    }
}

void LoopManager::updateWaterTank() {
    // Water tank monitoring is handled by the timer-based system
    if (waterTankTimer_) {
        (*waterTankTimer_)();
    } else {
        LOG(WARNING, "LoopManager: Water tank timer not initialized");
    }
}

void LoopManager::updateProcessControl() {
    // Cache millis() at start to avoid multiple calls
    const unsigned long  processStart = millis();
    const MachineStateId currentState = systemContext_.machineStateContext()->getCurrentStateId();

    using CleverCoffee::Timing::DEBUG_LOG_THROTTLE_MS;
    static unsigned long lastProcessControlLog = 0;
    const bool           shouldLog             = (processStart - lastProcessControlLog >= DEBUG_LOG_THROTTLE_MS);

    if (shouldLog) {
        LOGF(DEBUG,
             "updateProcessControl: state=%d, temp=%.1f°C, setpoint=%.1f°C",
             static_cast<int>(currentState),
             systemContext_.processTemperature(),
             systemContext_.processSetpoint());
        lastProcessControlLog = processStart;
    }

    processController_.updateProcessControl(currentState);
    const unsigned long processTime = millis() - processStart;

    if (processTime > 100) {
        LOGF(ERROR, "ProcessController update took %lums - blocking main loop!", processTime);
    } else if (processTime > 50) {
        LOGF(WARNING, "ProcessController update took %lums", processTime);
    }
}

void LoopManager::updateDisplay() {
    // Throttle debug logs to reduce log spam
    // Note: millis() not cached here as it's only used for throttling, not critical path
    using CleverCoffee::Timing::DEBUG_LOG_THROTTLE_MS;
    static unsigned long lastDisplayLog = 0;
    const unsigned long  now            = millis();
    if (now - lastDisplayLog >= DEBUG_LOG_THROTTLE_MS) {
        lastDisplayLog = now;
    }

    uiManager_.setUpdateRunning(false);

    if (Config::getInstance().hardwareOledEnabled.get()) {
        // Check if display should be turned off (after standby + display-off countdown)
        if (systemContext_.standbyCoordinator().shouldTurnOffDisplay()) {
            if (U8G2* display = systemContext_.hardwareContext().display()) {
                display->setPowerSave(1); // Turn off display to save power
            }
            return;                       // Don't update display while it's off
        }

        // Sync UIManager buffer ready flag with UICoordinator flag
        // The display template sets the coordinator flag, we need to sync it to UIManager
        if (systemContext_.uiCoordinator().isDisplayBufferReady() && !uiManager_.isBufferReady()) {
            uiManager_.setBufferReady(true);
        }

        // Check if we're in a brew state - display updates are critical during brewing
        const auto currentState = systemContext_.machineStateContext()->getCurrentStateId();
        const bool isBrewActive = isBrewState(currentState) && currentState != MachineStateId::BREW_FINISHED;

        // During brew, update display more aggressively (only check standby, ignore network conditions)
        // Outside brew, use normal conditions to avoid conflicts with network operations
        bool canUpdate = false;
        if (isBrewActive) {
            // During brew: only check standby condition, always update if buffer ready
            bool standbyCondition = (!Config::getInstance().standbyEnabled.get() ||
                                     systemContext_.standbyCoordinator().getRemainingTimeMillis() > 0);
            canUpdate             = standbyCondition;
        } else {
            // Normal operation: check all conditions
            bool websiteCondition = !systemContext_.uiCoordinator().isWebsiteUpdateRunning();
            bool mqttCondition    = (!systemContext_.mqttManager() || !systemContext_.mqttManager()->isUpdateRunning());
            bool hassioCondition  = !systemContext_.uiCoordinator().isHassioUpdateRunning();
            bool standbyCondition = (!Config::getInstance().standbyEnabled.get() ||
                                     systemContext_.standbyCoordinator().getRemainingTimeMillis() > 0);
            canUpdate             = websiteCondition && mqttCondition && hassioCondition && standbyCondition;
        }

        // Update display if conditions are met and buffer is ready
        if (canUpdate && uiManager_.isBufferReady()) {
            uiManager_.forceUpdate();
            uiManager_.setBufferReady(false);
            systemContext_.setDisplayBufferReady(false); // Clear coordinator flag too
            uiManager_.setUpdateRunning(true);
        }
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
        using CleverCoffee::Timing::DISPLAY_REFRESH_INTERVAL_MS;
        using CleverCoffee::Timing::HASSIO_DISCOVERY_INTERVAL_MS;
        hassioDiscoveryTimer_ =
            std::make_unique<MillisecondTimer>([this]() { sendHASSIODiscoveryMsg(systemContext_); },
                                               std::chrono::milliseconds(HASSIO_DISCOVERY_INTERVAL_MS));
        printDisplayTimer_ = std::make_unique<MillisecondTimer>(DisplayTemplateManager::printScreen,
                                                                std::chrono::milliseconds(DISPLAY_REFRESH_INTERVAL_MS));
        LOG(INFO, "LoopManager: General timers initialized");

        // 2. Water tank monitoring timer
        using CleverCoffee::Timing::WATER_TANK_CHECK_INTERVAL_MS;
        waterTankTimer_ = std::make_unique<MillisecondTimer>(std::bind(&LoopManager::checkWaterTankLevel, this),
                                                             std::chrono::milliseconds(WATER_TANK_CHECK_INTERVAL_MS));
        if (!waterTankTimer_) {
            LOG(ERROR, "LoopManager: Failed to create water tank timer");
            return false;
        }
        LOG(INFO, "LoopManager: Water tank timer initialized (200ms interval)");

        // 3. Centralized sensor timers
        using CleverCoffee::Timing::PRESSURE_SENSOR_INTERVAL_MS;
        using CleverCoffee::Timing::SCALE_SENSOR_INTERVAL_MS;
        using CleverCoffee::Timing::TEMPERATURE_SENSOR_INTERVAL_MS;
        // Temperature sensor timer (2.5Hz for PID stability, Dallas DS18B20 takes ~100ms to read)
        temperatureTimer_ =
            std::make_unique<MillisecondTimer>(std::bind(&LoopManager::updateTemperatureSensor, this),
                                               std::chrono::milliseconds(TEMPERATURE_SENSOR_INTERVAL_MS));

        // Pressure sensor timer (20Hz for responsiveness)
        pressureTimer_ = std::make_unique<MillisecondTimer>(std::bind(&LoopManager::updatePressureSensor, this),
                                                            std::chrono::milliseconds(PRESSURE_SENSOR_INTERVAL_MS));

        // Scale sensor timer (10Hz for good balance)
        scaleTimer_ = std::make_unique<MillisecondTimer>(std::bind(&LoopManager::updateScaleSensor, this),
                                                         std::chrono::milliseconds(SCALE_SENSOR_INTERVAL_MS));

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
    // SensorCoordinator auto-updates water tank sensor
    // Water tank status is accessed via sensorCoordinator_.isWaterTankFull() directly
}

void LoopManager::updateTemperatureSensor() {
    // SensorCoordinator auto-updates temperature sensor - no manual update needed
    // Temperature updates are handled by SensorCoordinator.update() called in the main loop
    temperatureUpdateCount_++;
}

void LoopManager::updatePressureSensor() {
    if (Config::getInstance().hardwareSensorsPressureEnabled.get()) {
        // SensorCoordinator auto-updates pressure sensor

        pressureUpdateCount_++;
    }
}

void LoopManager::updateScaleSensor() {
    if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        const unsigned long startTime = millis();

        // Update current reading weight from SensorCoordinator

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
    // Simple state machine: start tracking when brew starts, stop when brew ends
    const auto currentState = systemContext_.machineStateContext()->getCurrentStateId();
    const bool isBrewActive = isBrewState(currentState) && currentState != MachineStateId::BREW_FINISHED;

    // Check if we need to start brew weight tracking
    if (isBrewActive && !sensorCoordinator_.isBrewWeightTrackingActive()) {
        sensorCoordinator_.startBrewWeightTracking();
    }
    // Check if we need to stop brew weight tracking
    else if (!isBrewActive && sensorCoordinator_.isBrewWeightTrackingActive()) {
        sensorCoordinator_.stopBrewWeightTracking();
    }
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
    // Update WiFi connection
    if (auto* wifiManager = systemContext_.cleverCoffeeWiFiManager()) {
        wifiManager->checkAndMaintainConnection();
    }

    // Update MQTT connection
    if (auto* mqttManager = systemContext_.mqttManager()) {
        mqttManager->checkConnection();
        mqttManager->loop();

        // Additional MQTT operations that need system context
        if (mqttManager->isEnabled() && mqttManager->isConnected()) {
            mqttManager->setUpdateRunning(false);

            if (systemContext_.cleverCoffeeWiFiManager() &&
                systemContext_.cleverCoffeeWiFiManager()->getSignalStrength() > 1) {
                // Note: Sensor updates are now automatic and non-blocking, so no need to check tempNotRunning
                bool displayBufferNotReady = !systemContext_.uiCoordinator().isDisplayBufferReady();
                if (displayBufferNotReady) {
                    mqttManager->writeSysParamsToMQTT(true);
                }
            }

            // Home Assistant discovery (handled by timer callback)
            // Timer will call sendHASSIODiscoveryMsg(systemContext_) when ready
        }
    }

    // Handle OTA updates
    ArduinoOTA.handle();

    // Reset WiFi reconnection counter on successful connection
    if (WiFi.status() == WL_CONNECTED && !systemContext_.networkCoordinator().isOfflineMode()) {
        systemContext_.networkCoordinator().resetWifiReconnects();
    }
}

void LoopManager::updateWebsite() {
    // Simplified website coordination - delegate to WebServerManager
    // Note: Sensor updates are now automatic and non-blocking, so no need to check tempNotRunning
    // Cache millis() to avoid multiple calls
    const unsigned long now = millis();

    bool hassioNotRunning      = !systemContext_.uiCoordinator().isHassioUpdateRunning();
    bool displayBufferNotReady = !systemContext_.uiCoordinator().isDisplayBufferReady();

    const bool canSendData = (now - lastTempEvent_) > tempEventInterval_ &&
                             (!systemContext_.mqttManager() || !systemContext_.mqttManager()->isUpdateRunning()) &&
                             hassioNotRunning && displayBufferNotReady;

    // Check offline mode from NetworkCoordinator
    bool isOfflineMode = systemContext_.networkCoordinator().isOfflineMode();

    if (canSendData && WiFi.status() == WL_CONNECTED && !isOfflineMode) {
        systemContext_.uiCoordinator().setWebsiteUpdateRunning(true);

        // Delegate to WebServerManager for actual transmission
        if (systemContext_.webServerManager()) {
            systemContext_.webServerManager()->sendTempEvent(systemContext_.processTemperature(),
                                                             Config::getInstance().brewSetpoint.get(),
                                                             systemContext_.processController()->getPIDOutput() /
                                                                 10); // Convert promill to percent

            if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
                systemContext_.webServerManager()->sendWeightEvent();
            }
        }

        lastTempEvent_ = now;
        systemContext_.uiCoordinator().setWebsiteUpdateRunning(false);
    }
}

// Old updateSensors() method removed - replaced with centralized timer system

void LoopManager::updateSwitchesAndStandby() {
    // Switch handling and standby management extracted from loopPid()
    // All handlers are required and always exist
    systemContext_.steamHandler().process();
    systemContext_.powerHandler().process();

    // Update standby timer through coordinator
    systemContext_.standbyCoordinator().update();
}

void LoopManager::updateStateMachine() {
    // State machine updates extracted from loopPid()
    extern std::unique_ptr<StateMachine> stateMachine;

    // Debug: Check if state machine exists
    if (!stateMachine) {
        LOG(ERROR, "CRITICAL: stateMachine is nullptr!");
        return;
    }

    // Debug: Check initialization
    if (!stateMachine->isInitialized()) {
        LOG(WARNING, "StateMachine not initialized yet");
        return;
    }

    // Get state BEFORE update
    const MachineStateId stateBefore     = stateMachine->getCurrentStateId();
    const char*          stateNameBefore = stateMachine->getCurrentStateName();

    // Update state machine (replaces handleMachineState())
    stateMachine->update();

    // Get state AFTER update to detect changes
    const MachineStateId newState       = stateMachine->getCurrentStateId();
    const char*          stateNameAfter = stateMachine->getCurrentStateName();

    // Log all state updates for debugging (throttled to reduce log spam)
    using CleverCoffee::Timing::DEBUG_LOG_THROTTLE_MS;
    static unsigned long lastStateMachineLog = 0;
    const unsigned long  now                 = millis();
    if (now - lastStateMachineLog >= DEBUG_LOG_THROTTLE_MS) {
        LOGF(DEBUG, "StateMachine::update() -> State: %s (%d)", stateNameAfter, static_cast<int>(newState));
        lastStateMachineLog = now;
    }

    // Check for state changes
    if (newState != systemContext_.machineStateContext()->getCurrentStateId()) {
        const auto oldState = systemContext_.machineStateContext()->getCurrentStateId();
        systemContext_.machineStateContext()->setCurrentStateId(newState);
        LOGF(WARNING,
             "State transition detected: %d -> %d (%s -> %s)",
             static_cast<int>(oldState),
             static_cast<int>(newState),
             stateNameBefore,
             stateNameAfter);
    }

    // Update handlers - all handlers are required and always exist
    systemContext_.hotWaterHandler().process();
    systemContext_.brewHandler().process();
    systemContext_.brewHandler().valveSafetyShutdownCheck();

    // Update brew timer display state using UIManager - always available
    if (Config::getInstance().hardwareSwitchesBrewEnabled.get()) {
        uiManager_.shouldDisplayBrewTimer();
    } else {
        // Use inline function from displayCommon.h
        shouldDisplayBrewTimer(systemContext_);
    }
}

void LoopManager::configureSensorTimers(unsigned long temperatureIntervalMs,
                                        unsigned long pressureIntervalMs,
                                        unsigned long scaleIntervalMs) {
    try {
        // Recreate timer objects with new intervals
        temperatureTimer_ = std::make_unique<MillisecondTimer>(std::bind(&LoopManager::updateTemperatureSensor, this),
                                                               std::chrono::milliseconds(temperatureIntervalMs));

        pressureTimer_ = std::make_unique<MillisecondTimer>(std::bind(&LoopManager::updatePressureSensor, this),
                                                            std::chrono::milliseconds(pressureIntervalMs));

        scaleTimer_ = std::make_unique<MillisecondTimer>(std::bind(&LoopManager::updateScaleSensor, this),
                                                         std::chrono::milliseconds(scaleIntervalMs));

        // Reset counters when reconfiguring
        temperatureUpdateCount_ = 0;
        pressureUpdateCount_    = 0;
        scaleUpdateCount_       = 0;
        lastTimerLogTime_       = millis();

        LOGF(
            INFO,
            "Sensor timers reconfigured - Temperature: %lums (%.1fHz), Pressure: %lums (%.1fHz), Scale: %lums (%.1fHz)",
            temperatureIntervalMs,
            1000.0f / temperatureIntervalMs,
            pressureIntervalMs,
            1000.0f / pressureIntervalMs,
            scaleIntervalMs,
            1000.0f / scaleIntervalMs);
    } catch (const std::exception& e) {
        LOGF(ERROR, "LoopManager: Exception reconfiguring sensor timers: %s", e.what());
    }
}

void LoopManager::logTimerConfiguration() const {
    const unsigned long currentTime     = millis();
    const float         timeDiffSeconds = (currentTime - lastTimerLogTime_) / 1000.0f;

    if (timeDiffSeconds > 0) {
        const float actualTempFreq     = temperatureUpdateCount_ / timeDiffSeconds;
        const float actualPressureFreq = pressureUpdateCount_ / timeDiffSeconds;
        const float actualScaleFreq    = scaleUpdateCount_ / timeDiffSeconds;

        LOGF(DEBUG, "Centralized Sensor Timer Performance Report:");
        LOGF(DEBUG, "  Temperature: %.1fHz actual (%lu updates)", actualTempFreq, temperatureUpdateCount_);
        LOGF(DEBUG, "  Pressure: %.1fHz actual (%lu updates)", actualPressureFreq, pressureUpdateCount_);
        LOGF(DEBUG, "  Scale: %.1fHz actual (%lu updates)", actualScaleFreq, scaleUpdateCount_);
    }

    // Reset counters for next measurement period
    temperatureUpdateCount_ = 0;
    pressureUpdateCount_    = 0;
    scaleUpdateCount_       = 0;
}
