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

#include "hardware/pressureSensor.h"
#include "isr.h"
#include "utils/timingDebug.h"
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

#include "brewHandler.h"
#include "hotWaterHandler.h"
#include "standby.h"

// External functions from other modules
extern void serverSetup();

#include "display/displayTemplateManager.h"

#include "powerHandler.h"
#include "scaleHandler.h"
#include "steamHandler.h"

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

        const System::DisplayTemplate templateId = Config::getInstance().displayTemplate.get();
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
    if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
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
        g_state.process.aggbKi = (Config::getInstance().pidBdTn.get() == 0) ? 0 : Config::getInstance().pidBdKp.get() / Config::getInstance().pidBdTn.get();
        g_state.process.aggbKd = Config::getInstance().pidBdTv.get() * Config::getInstance().pidBdKp.get();
        g_state.process.aggKi = (Config::getInstance().pidRegularTn.get() == 0) ? 0 : Config::getInstance().pidRegularKp.get() / Config::getInstance().pidRegularTn.get();
        g_state.process.aggKd = Config::getInstance().pidRegularTv.get() * Config::getInstance().pidRegularKp.get();

        // Set PID tunings now that parameters are calculated
        g_state.pid->SetTunings(Config::getInstance().pidRegularKp.get(), g_state.process.aggKi, g_state.process.aggKd, 1);

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
            g_state.timing.loopWaterTank = std::make_unique<Timer>(
                []() {
                    if (sensorManager) {
                        sensorManager->updateWaterTankSensor();
                        g_state.machine.waterTankFull = sensorManager->isWaterTankFull();
                    }
                },
                200);
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
        if (processController) {
            processController->updateProcessControl(static_cast<int>(g_state.machine.machineState));
        }
        else {
            LOG(ERROR, "ProcessController not available - temperature control may not function correctly");
        }

        // LED updates are handled by LoopManager (when available)

        // print timing related data to check what is causing stutters
        debugTimingLoop();
    }
}

// Missing function implementations for legacy compatibility
// Note: brew(), manualFlush(), and checkBrewActive() are already defined in brewUtils.h

// Legacy wrapper functions for external modules that haven't been updated yet
void checkWaterTank() {
    if (sensorManager) {
        sensorManager->updateWaterTankSensor();
        g_state.machine.waterTankFull = sensorManager->isWaterTankFull();
    }
}

void checkWifi() {
    if (g_state.network.cleverCoffeeWiFiManager) {
        g_state.network.cleverCoffeeWiFiManager->checkAndMaintainConnection();
    }
}

void performSafeShutdown() {
    if (processController) {
        processController->performSafeShutdown();
    }
}
