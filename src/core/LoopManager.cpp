/**
 * @file LoopManager.cpp
 * @brief Implementation of LoopManager for main loop coordination
 */

#include "LoopManager.h"
#include "../control/ProcessController.h"
#include "../sensors/SensorManager.h"
#include "../ui/UIManager.h"
#include "../Config.h"
#include "../GlobalVariables.h"
#include "Logger.h"
#include "../hardware/Relay.h"
#include "../network/MQTTManager.h"
#include "../utils/Timer.h"
#include <Arduino.h>

// Forward declaration for debug timing function
extern void debugTimingLoop();

// Forward declarations for external variables needed for display updates
extern bool websiteUpdateRunning;
extern bool hassioUpdateRunning;
extern bool temperatureUpdateRunning;
extern bool displayBufferReady;
extern bool displayUpdateRunning;
extern U8G2* u8g2;

// External timer for display updates - Timer object from main.cpp
extern Timer printDisplayTimer;

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
extern bool waterTankFull;
extern LegacyMachineState machineState;
extern double temperature;
extern double setpoint;

// Display-related external declarations
extern Timer printDisplayTimer;
extern unsigned long standbyModeRemainingTimeMillis;
extern U8G2* u8g2;
extern std::unique_ptr<MQTTManager> mqttManager;

// Hardware components for LED control
extern std::unique_ptr<Relay> statusLed;
extern std::unique_ptr<Relay> brewLed;
extern std::unique_ptr<Relay> steamLed;

LoopManager::LoopManager(ProcessController* processController,
                        SensorManager* sensorManager,
                        UIManager* uiManager)
    : processController_(processController),
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
    
    // 5. Print timing related data to check what is causing stutters
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
            LOGF(DEBUG, "LoopManager: Processed %lu loops, max duration: %lums", 
                loopCount_, maxLoopTime_);
        }
    }
}

void LoopManager::updateLEDs() {
    // Status LED - indicates when temperature is reached
    if (Config::getInstance().get<bool>("hardware.leds.status.enabled") && statusLed != nullptr) {
        bool shouldTurnOn = false;
        
        // Turn on when at target temperature (normal or steam mode)
        if ((machineState == kPidNormal && (fabs(temperature - setpoint) < 0.3)) || 
            (temperature > 115 && fabs(temperature - setpoint) < 5)) {
            shouldTurnOn = true;
        }
        
        if (shouldTurnOn) {
            statusLed->on();
        } else {
            statusLed->off();
        }
    }

    // Brew LED - indicates brewing state
    if (Config::getInstance().get<bool>("hardware.leds.brew.enabled") && brewLed != nullptr) {
        if (machineState == kBrew) {
            brewLed->on();
        } else {
            brewLed->off();
        }
    }

    // Steam LED - indicates steam mode
    if (Config::getInstance().get<bool>("hardware.leds.steam.enabled") && steamLed != nullptr) {
        if (machineState == kSteam) {
            steamLed->on();
        } else {
            steamLed->off();
        }
    }
}

void LoopManager::updateWaterTank() {
    // Water tank monitoring is handled by the timer-based system
    // The timer calls checkWaterTank() every 200ms automatically
    if (waterTankTimer_) {
        // Timer update is handled automatically by Timer class
        // No explicit action needed here
    } else {
        // Fallback: direct call to water tank check
        // This should normally not be needed if timer is set up correctly
        checkWaterTank();
    }
}

void LoopManager::updateProcessControl() {
    // Call the full loopPid() function which includes ProcessController
    // as well as display updates, MQTT, WiFi, OTA, and other critical logic
    extern void loopPid();
    loopPid();
}

void LoopManager::updateDisplay() {
    // Handle display updates similar to the original main loop logic
    if (uiManager_) {
        // Use UIManager for display management
        uiManager_->setUpdateRunning(false);
        
        if (Config::getInstance().get<bool>("hardware.oled.enabled")) {
            // Only update display on loops that have not had other major tasks running
            // and when not in standby display-off mode
            if (!websiteUpdateRunning && 
                (!mqttManager || !mqttManager->isUpdateRunning()) && 
                !hassioUpdateRunning && 
                !temperatureUpdateRunning && 
                (standbyModeRemainingTimeMillis > 0)) {
                
                if (uiManager_->isBufferReady()) {
                    uiManager_->forceUpdate();
                    uiManager_->setBufferReady(false);
                    uiManager_->setUpdateRunning(true);
                } else {
                    // This is the critical call that was missing!
                    // It triggers the display template rendering
                    printDisplayTimer();
                }
            }
        }
    } else {
        // Fallback to original display logic when UIManager is not available
        displayUpdateRunning = false;

        if (Config::getInstance().get<bool>("hardware.oled.enabled")) {
            // Only update display on loops that have not had other major tasks running
            // and when not in standby display-off mode
            if (!websiteUpdateRunning && 
                (!mqttManager || !mqttManager->isUpdateRunning()) && 
                !hassioUpdateRunning && 
                !temperatureUpdateRunning && 
                (standbyModeRemainingTimeMillis > 0)) {
                
                if (displayBufferReady) {
                    u8g2->sendBuffer();
                    displayBufferReady = false;
                    displayUpdateRunning = true;
                } else {
                    // This is the critical call that was missing!
                    // It triggers the display template rendering which sets displayBufferReady = true
                    printDisplayTimer();
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