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

// Modern sensor management
#include "sensors/SensorManager.h"
SensorManager* sensorManager = nullptr;

// Modern state machine
#include "state/StateMachine.h"
std::unique_ptr<StateMachine> stateMachine = nullptr;

// Modern process control
#include "control/ProcessController.h"
std::unique_ptr<ProcessController> processController = nullptr;

// Modern loop management
#include "core/LoopManager.h"
std::unique_ptr<LoopManager> loopManager = nullptr;

#include "handlers/BrewHandler.h"
#include "handlers/HotWaterHandler.h"
#include "handlers/PowerHandler.h"
#include "handlers/SteamHandler.h"
#include "standby.h"

#include "display/displayTemplateManager.h"

#include "scaleHandler.h"

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

    logMemoryBasic("Before HardwareManager Access");
    if (systemInitializer->getHardwareManager()) {
        HardwareManager* hwManager = systemInitializer->getHardwareManager();
        logMemoryBasic("After HardwareManager Retrieved");

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
    if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        if (sensorManager) {
            logMemoryBasic("Before Sensor Init scales");
            sensorManager->initializeScale();
            logMemoryBasic("After HardwareManager Access");
        }
    }

    g_state.machine.systemInitialized = systemInitializer->isInitialized();

    // Initialize modern state machine after all managers are set up
    if (g_state.machine.systemInitialized) {
        stateMachine = std::make_unique<StateMachine>(systemInitializer->getDisplayManager(), systemInitializer->getHardwareManager(), sensorManager, systemInitializer->getWiFiManager(), systemInitializer->getMQTTManager());

        InitHelpers::logInitResult("StateMachine", stateMachine->initialize());

        // Initialize PID parameters now that config is available
        // TODO remove?
        g_state.process.aggbKi = (Config::getInstance().pidBdTn.get() == 0) ? 0 : Config::getInstance().pidBdKp.get() / Config::getInstance().pidBdTn.get();
        g_state.process.aggbKd = Config::getInstance().pidBdTv.get() * Config::getInstance().pidBdKp.get();
        g_state.process.aggKi = (Config::getInstance().pidRegularTn.get() == 0) ? 0 : Config::getInstance().pidRegularKp.get() / Config::getInstance().pidRegularTn.get();
        g_state.process.aggKd = Config::getInstance().pidRegularTv.get() * Config::getInstance().pidRegularKp.get();

        // C++23 enhanced logging - shows PID parameters clearly
        LOGF(INFO, "PID initialized: Kp={:.3f}, Ki={:.3f}, Kd={:.3f}",
                  Config::getInstance().pidRegularKp.get(), g_state.process.aggKi, g_state.process.aggKd);

        // Set PID tunings now that parameters are calculated
        g_state.pid->SetTunings(Config::getInstance().pidRegularKp.get(), g_state.process.aggKi, g_state.process.aggKd, 1);

        // Initialize ProcessController for PID control
        processController = std::make_unique<ProcessController>(systemInitializer->getDisplayManager(), systemInitializer->getHardwareManager(), sensorManager, systemInitializer->getMQTTManager());

        g_state.coordination.processController = processController.get();

        InitHelpers::logInitResult("ProcessController", processController->initialize());

        // Initialize LoopManager for main loop coordination
        loopManager = std::make_unique<LoopManager>(processController.get(), sensorManager, systemInitializer->getUIManager());

        InitHelpers::logInitResult("LoopManager", loopManager->initialize());
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
