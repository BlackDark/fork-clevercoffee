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
#include "clevercoffee/Logger.h"
#include "clevercoffee/core/SystemInitializer.h"
#include "clevercoffee/network/CleverCoffeeWiFiManager.h"
#include "clevercoffee/network/MQTTManager.h"

#include <ArduinoOTA.h>
#include <LittleFS.h>
#include <PID_v1.h>  // for PID calculation
#include <Preferences.h>
#include <U8g2lib.h> // i2c display
#include <WiFiManager.h>
#include <os.h>

// Defaults
#include "clevercoffee/defaults.h"

// Includes
#include "clevercoffee/Config.h"
#include "clevercoffee/GlobalState.h"

// Utilities
#include "clevercoffee/utils/SystemUtils.h"
#include "clevercoffee/utils/helperUtils.h"
#include "clevercoffee/utils/memoryUtils.h"

// Hardware classes
#include "clevercoffee/hardware/GPIOPin.h"
#include "clevercoffee/hardware/IOSwitch.h"
#include "clevercoffee/hardware/LED.h"
#include "clevercoffee/hardware/Relay.h"
#include "clevercoffee/hardware/StandardLED.h"
#include "clevercoffee/hardware/Switch.h"
#include "clevercoffee/hardware/pinmapping.h"
#include "clevercoffee/hardware/pressureSensor.h"
#include "clevercoffee/hardware/tempsensors/TempSensorDallas.h"
#include "clevercoffee/hardware/tempsensors/TempSensorTSIC.h"
#include "clevercoffee/isr.h"

#include <Wire.h>

// System initializer
std::unique_ptr<SystemInitializer> systemInitializer = nullptr;

// Display Manager
#include "clevercoffee/display/DisplayManager.h"
std::unique_ptr<DisplayManager> displayManager = nullptr;

// Hardware Manager
#include "clevercoffee/hardware/HardwareManager.h"
std::unique_ptr<HardwareManager> hardwareManager = nullptr;

// Modern sensor management
#include "clevercoffee/sensors/SensorManager.h"
SensorManager* sensorManager = nullptr;

// Modern state machine
#include "clevercoffee/state/StateMachine.h"
std::unique_ptr<StateMachine> stateMachine = nullptr;

// Modern process control
#include "clevercoffee/control/ProcessController.h"
std::unique_ptr<ProcessController> processController = nullptr;

// Modern loop management
#include "clevercoffee/core/LoopManager.h"
std::unique_ptr<LoopManager> loopManager = nullptr;

#include "clevercoffee/display/displayTemplateManager.h"
#include "clevercoffee/handlers/BrewHandler.h"
#include "clevercoffee/handlers/HotWaterHandler.h"
#include "clevercoffee/handlers/PowerHandler.h"
#include "clevercoffee/handlers/SteamHandler.h"
#include "clevercoffee/scaleHandler.h"
#include "clevercoffee/standby.h"

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
} // namespace InitHelpers

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
    SensorManager*           sensorManager   = systemInitializer->getSensorManager();
    DisplayManager*          displayManager  = systemInitializer->getDisplayManager();
    HardwareManager*         hardwareManager = systemInitializer->getHardwareManager();
    CleverCoffeeWiFiManager* wifiManager     = systemInitializer->getWiFiManager();
    MQTTManager*             mqttManager     = systemInitializer->getMQTTManager();
    UIManager*               uiManager       = systemInitializer->getUIManager();

    // Complete initialization steps that require global dependencies
    if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        if (sensorManager) {
            logMemoryBasic("Before Sensor Init scales");
            sensorManager->initializeScale();
            logMemoryBasic("After HardwareManager Access");
        }
    }

    if (systemInitializer->isInitialized()) {
        stateMachine = std::make_unique<StateMachine>(
            *systemInitializer->getSystemContext(), displayManager, hardwareManager, sensorManager, wifiManager,
            mqttManager);
        InitHelpers::logInitResult("StateMachine", stateMachine->initialize());

        // Initialize ProcessController for PID control
        processController =
            std::make_unique<ProcessController>(displayManager, hardwareManager, sensorManager, mqttManager);
        g_state.coordination.processController = processController.get(); // Still needed for now
        InitHelpers::logInitResult("ProcessController", processController->initialize());

        // Initialize LoopManager for main loop coordination
        loopManager = std::make_unique<LoopManager>(processController.get(), sensorManager, uiManager);
        InitHelpers::logInitResult("LoopManager", loopManager->initialize());

        // Configure sensor update timers (uncomment and modify as needed)
        // loopManager->configureSensorTimers(100, 50, 100); // Temperature: 100ms (10Hz), Pressure: 50ms (20Hz), Scale: 100ms (10Hz)
        // loopManager->configureSensorTimers(200, 100, 200); // Slower: Temperature: 200ms (5Hz), Pressure: 100ms (10Hz), Scale: 200ms (5Hz)
        // loopManager->configureSensorTimers(50, 25, 50); // Faster: Temperature: 50ms (20Hz), Pressure: 25ms (40Hz), Scale: 50ms (20Hz)
    }

    // Initialize handler objects and set up references in global state
    initializeHandlers();
    InitHelpers::logInitResult("Handlers", true);

    logMemory("Setup Complete");
    LOGF(INFO, "System setup completed via SystemInitializer - CleverCoffee ready!");
}

void loop() {
    if (loopManager) {
        // Use modern LoopManager for coordinated main loop updates
        loopManager->update();
    }
}
