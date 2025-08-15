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
#include "clevercoffee/utils/ModernTimer.h"
#include "clevercoffee/utils/helperUtils.h"
#include "clevercoffee/utils/SystemUtils.h"
#include "clevercoffee/utils/memoryUtils.h"

// Hardware classes
#include "clevercoffee/hardware/GPIOPin.h"
#include "clevercoffee/hardware/IOSwitch.h"
#include "clevercoffee/hardware/LED.h"
#include "clevercoffee/hardware/Relay.h"
#include "clevercoffee/hardware/StandardLED.h"
#include "clevercoffee/hardware/Switch.h"
#include "clevercoffee/hardware/pinmapping.h"
#include "clevercoffee/hardware/tempsensors/TempSensorDallas.h"
#include "clevercoffee/hardware/tempsensors/TempSensorTSIC.h"

#include "clevercoffee/hardware/pressureSensor.h"
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

#include "clevercoffee/handlers/BrewHandler.h"
#include "clevercoffee/handlers/HotWaterHandler.h"
#include "clevercoffee/handlers/PowerHandler.h"
#include "clevercoffee/handlers/SteamHandler.h"
#include "clevercoffee/standby.h"

#include "clevercoffee/display/displayTemplateManager.h"

#include "clevercoffee/scaleHandler.h"

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

        // Hardware switch and sensor assignments
        g_state.hardware.powerSwitch = hwManager->getPowerSwitch();
        g_state.hardware.brewSwitch = hwManager->getBrewSwitch();
        g_state.hardware.steamSwitch = hwManager->getSteamSwitch();
        g_state.hardware.hotWaterSwitch = hwManager->getHotWaterSwitch();
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
