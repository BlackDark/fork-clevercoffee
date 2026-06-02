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

// Hardware Manager (must be before SystemInitializer to resolve forward declaration)
#include "clevercoffee/hardware/HardwareManager.h"

// Libraries & Dependencies
#include "clevercoffee/Logger.h"
#include "clevercoffee/context/SystemContext.h"
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
#include "clevercoffee/types/GlobalTypes.h"

// Utilities
#include "clevercoffee/utils/Resilience.h"
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

// Hardware Manager (already included at top)
std::unique_ptr<CleverCoffee::HardwareManager> hardwareManager = nullptr;

// Modern state machine
#include "clevercoffee/state/StateMachine.h"
std::unique_ptr<StateMachine> stateMachine = nullptr;

// Modern process control
#include "clevercoffee/control/ProcessController.h"
std::unique_ptr<ProcessController> processController = nullptr;

// Modern loop management
#include "clevercoffee/core/LoopManager.h"
std::unique_ptr<LoopManager> loopManager = nullptr;

#include "clevercoffee/display/DisplayTemplateManager.h"

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

// Watchdog timer for system hang detection (5 second timeout)
// If the main loop hangs, the watchdog will reset the system for safety
static Watchdog g_watchdog(5000);

void setup() {
    logMemory("Setup Start");

    // Initialize watchdog early - will reset system if setup hangs
    g_watchdog.begin();
    LOG(INFO, "Watchdog timer initialized - system will reset if main loop hangs");

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

    LOGF(DEBUG,
         "SystemInitializer::initialize() check: returned=%d, isInitialized=%d",
         true,
         systemInitializer->isInitialized());

    // Get managers from SystemInitializer - they are owned by systemInitializer
    // All managers are REQUIRED and always exist
    auto& displayManager = systemInitializer->getDisplayManager();
    auto& wifiManager    = systemInitializer->getWiFiManager();
    auto& mqttManager    = systemInitializer->getMQTTManager();
    auto& oledDriver     = systemInitializer->getOledDriver();
    // Note: HardwareManager and SystemContext are also accessed via references in constructors

    // Complete initialization steps that require global dependencies
    if (Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        // Get sensor coordinator from system context for scale initialization
        CleverCoffee::SensorCoordinator* sensorCoord = &systemInitializer->getSystemContext().sensorCoordinator();
        // Scale initialization will be handled via SensorCoordinator when Scale implements ISensor
        logMemoryBasic("Scale sensor support via SensorCoordinator");
    }

    if (systemInitializer->isInitialized()) {
        LOGF(INFO,
             "systemInitializer->isInitialized() = true, creating StateMachine, ProcessController, and LoopManager");

        // Get required components (references - guaranteed to exist)
        auto& systemContext   = systemInitializer->getSystemContext();
        auto& hardwareManager = systemInitializer->getHardwareManager();

        // Create StateMachine with required components as references
        stateMachine =
            std::make_unique<StateMachine>(systemContext, hardwareManager, displayManager, wifiManager, mqttManager);
        stateMachine->initialize(); // Always succeeds - uses fallback if needed
        InitHelpers::logInitResult("StateMachine", true);

        // Register MachineStateContext in SystemContext for safe access
        systemContext.setMachineStateContext(&stateMachine->getContext());

        // Finalize machine state (must be done after MachineStateContext is registered)
        (void)systemInitializer->finalizeMachineState();

        // Initialize ProcessController for PID control with required components as references
        processController = std::make_unique<ProcessController>(
            Config::getInstance(), systemContext, hardwareManager, displayManager, mqttManager);

        // Register ProcessController in SystemContext for safe access
        systemContext.setProcessController(processController.get());
        InitHelpers::logInitResult("ProcessController", processController->initialize());

        // Initialize LoopManager for main loop coordination with required components as references
        auto& sensorCoord = systemContext.sensorCoordinator();
        loopManager       = std::make_unique<LoopManager>(
            systemContext, hardwareManager, *processController, sensorCoord, oledDriver, stateMachine.get());
        InitHelpers::logInitResult("LoopManager", loopManager->initialize());

        // Configure sensor update timers (uncomment and modify as needed)
        // loopManager->configureSensorTimers(100, 50, 100); // Temperature: 100ms (10Hz), Pressure: 50ms (20Hz), Scale:
        // 100ms (10Hz) loopManager->configureSensorTimers(200, 100, 200); // Slower: Temperature: 200ms (5Hz),
        // Pressure: 100ms (10Hz), Scale: 200ms (5Hz) loopManager->configureSensorTimers(50, 25, 50); // Faster:
        // Temperature: 50ms (20Hz), Pressure: 25ms (40Hz), Scale: 50ms (20Hz)
    } else {
        // CRITICAL: This should never happen!
        LOG(FATAL, "CRITICAL: systemInitializer->isInitialized() returned false!");
        LOG(FATAL, "This means LoopManager was NOT created");
        LOG(FATAL, "Check if SystemInitializer::initialize() is setting systemInitialized_ = true");
        // Without LoopManager, the main loop will crash
    }

    systemInitializer->getSystemContext().markReady();

    logMemory("Setup Complete");
    LOGF(INFO, "System setup completed via SystemInitializer - CleverCoffee ready!");
}

void loop() {
    // Feed watchdog at the start of each loop iteration
    // This ensures system resets if the loop hangs
    g_watchdog.feed();

    // DEBUG: Log loop iteration with ISR status
    static unsigned long loopCount    = 0;
    static unsigned long lastDebugLog = millis();

    loopCount++;

    if (loopManager) {
        // Use modern LoopManager for coordinated main loop updates
        loopManager->update();

        // Log ISR and loop status every 5 seconds
        unsigned long now = millis();
        if (now - lastDebugLog >= 5000) {
            // ISR counters are declared in isr.h
            LOGF(DEBUG,
                 "LOOP STATUS: loops=%lu, ISR enabled=%d, ISR calls=%lu, relay_on=%lu, relay_off=%lu, temp=%.1f°C, "
                 "setpoint=%.1f°C, pidOutput=%.1f",
                 loopCount,
                 (int)isr_enabled.load(),
                 isr_call_count.load(),
                 isr_relay_on_count.load(),
                 isr_relay_off_count.load(),
                 systemInitializer->getSystemContext().processTemperature(),
                 systemInitializer->getSystemContext().processSetpoint(),
                 systemInitializer->getSystemContext().processPidOutput());

            lastDebugLog = now;
        }
    } else {
        LOG(ERROR, "CRITICAL: LoopManager is nullptr!");
    }
}
