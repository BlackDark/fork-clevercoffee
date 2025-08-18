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
#include <string_view>

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
#include "utils/ModernTimer.h"
#include "utils/helperUtils.h"
#include "utils/SystemUtils.h"
#include "utils/memoryUtils.h"

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

#include "hardware/pressureSensor.h"
#include "isr.h"
#include <Wire.h>

// System initializer
std::unique_ptr<SystemInitializer> systemInitializer = nullptr;

// Display Manager
#include "display/DisplayManager.h"
std::unique_ptr<DisplayManager> displayManager = nullptr;

// Hardware Manager
#include "hardware/HardwareManager.h"
std::unique_ptr<HardwareManager> hardwareManager = nullptr;

// Modern state machine
#include "state/StateMachine.h"
#include "sensors/SensorManager.h"
std::unique_ptr<StateMachine> stateMachine = nullptr;

// Modern process control
#include "control/ProcessController.h"
std::unique_ptr<ProcessController> processController = nullptr;

// Modern loop management
#include "core/LoopManager.h"
std::unique_ptr<LoopManager> loopManager = nullptr;

#include "brewHandler.h"
#include "hotWaterHandler.h"
#include "standby.h"

#include "display/displayTemplateManager.h"

#include "powerHandler.h"
#include "scaleHandler.h"
#include "steamHandler.h"

// Modern C++ initialization helpers
namespace InitHelpers {
    /**
     * @brief Check if initialization was successful with detailed logging
     * @param component Component name for logging
     * @param success Success status
     * @return Success status
     */
    inline bool logInitResult(const char* component, bool success) noexcept {
        if (success) {
            LOGF(INFO, "%s initialized successfully", component);
        } else {
            LOGF(ERROR, "%s initialization failed!", component);
        }
        return success;
    }
}

void setup() {
    logMemory("Setup Start");

    // Initialize system using RAII SystemInitializer
    logMemoryBasic("Before SystemInitializer");
    systemInitializer = std::make_unique<SystemInitializer>();

    logMemoryBasic("Before SystemInitializer->initialize()");

    // Traditional boolean error handling
    if (!systemInitializer->initialize()) {
        logMemory("SystemInitializer FAILED");
        LOGF(ERROR, "System initialization failed! Check hardware connections.");
        Serial.println("Critical system initialization error detected!");
        Serial.flush();
        exit(0);
    }

    logMemoryBasic("After SystemInitializer->initialize()");

    // Get managers from SystemInitializer - they are owned by systemInitializer
    SensorManager* sensorManager = systemInitializer->getSensorManager();
    DisplayManager* displayManager = systemInitializer->getDisplayManager();
    HardwareManager* hardwareManager = systemInitializer->getHardwareManager();
    CleverCoffeeWiFiManager* wifiManager = systemInitializer->getWiFiManager();
    MQTTManager* mqttManager = systemInitializer->getMQTTManager();
    UIManager* uiManager = systemInitializer->getUIManager();


    // Complete initialization steps that require global dependencies
    if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        if (sensorManager) {
            logMemoryBasic("Before Sensor Init scales");
            sensorManager->initializeScale();
            logMemoryBasic("After HardwareManager Access");
        }
    }

    // Initialize modern state machine after all managers are set up
    if (systemInitializer->isInitialized()) {
        stateMachine = std::make_unique<StateMachine>(displayManager, hardwareManager, sensorManager, wifiManager, mqttManager);
        InitHelpers::logInitResult("StateMachine", stateMachine->initialize());

        // Initialize ProcessController for PID control
        processController = std::make_unique<ProcessController>(displayManager, hardwareManager, sensorManager, mqttManager);
        g_state.coordination.processController = processController.get(); // Still needed for now
        InitHelpers::logInitResult("ProcessController", processController->initialize());

        // Initialize LoopManager for main loop coordination
        loopManager = std::make_unique<LoopManager>(processController.get(), sensorManager, uiManager);
        InitHelpers::logInitResult("LoopManager", loopManager->initialize());
    }

    logMemory("Setup Complete");
    LOGF(INFO, "System setup completed via SystemInitializer - CleverCoffee ready!");
}

void loop() {
    if (loopManager) {
        // Use modern LoopManager for coordinated main loop updates
        loopManager->update();
    }
}
