/**
 * @file SystemInitializer.h
 * @brief RAII wrapper for system initialization
 */

#pragma once

#include <memory>
#include <functional>
#include <Arduino.h>

// Forward declarations
class DisplayManager;
class HardwareManager;
class MQTTManager;
class SensorManager;

/**
 * @class SystemInitializer
 * @brief RAII wrapper for complete system initialization
 *
 * This class provides safe management of system initialization using RAII principles.
 * It encapsulates all initialization phases with proper error handling and logging.
 */
class SystemInitializer {
    public:
        /**
         * @brief Constructor - initializes system initializer
         */
        SystemInitializer();

        /**
         * @brief Destructor - automatically cleans up resources
         */
        ~SystemInitializer() = default;

        // Disable copy constructor and assignment operator
        SystemInitializer(const SystemInitializer&) = delete;
        SystemInitializer& operator=(const SystemInitializer&) = delete;

        // Enable move constructor and assignment operator
        SystemInitializer(SystemInitializer&&) = default;
        SystemInitializer& operator=(SystemInitializer&&) = default;

        /**
         * @brief Initialize complete system
         * @return true if initialization successful
         */
        bool initialize();

        /**
         * @brief Check if system is initialized
         * @return true if system is ready
         */
        bool isInitialized() const {
            return systemInitialized_;
        }

        /**
         * @brief Get display manager
         * @return Pointer to display manager (may be null)
         */
        DisplayManager* getDisplayManager() const {
            return displayManager_.get();
        }

        /**
         * @brief Get hardware manager
         * @return Pointer to hardware manager (may be null)
         */
        HardwareManager* getHardwareManager() const {
            return hardwareManager_.get();
        }

        /**
         * @brief Get MQTT manager
         * @return Pointer to MQTT manager (may be null)
         */
        MQTTManager* getMQTTManager() const {
            return mqttManager_.get();
        }

        /**
         * @brief Get sensor manager
         * @return Pointer to sensor manager (may be null)
         */
        SensorManager* getSensorManager() const {
            return sensorManager_.get();
        }

    private:
        // Initialization state
        bool systemInitialized_;
        String hostname_;
        
        // Manager instances
        std::unique_ptr<DisplayManager> displayManager_;
        std::unique_ptr<HardwareManager> hardwareManager_;
        std::unique_ptr<MQTTManager> mqttManager_;
        std::unique_ptr<SensorManager> sensorManager_;

        // Initialization phases
        bool initializeLogger();
        bool initializeConfiguration();
        bool initializeDisplay();
        bool initializeHardware();
        bool initializeNetworking();
        bool initializeMQTT();
        bool initializePID();
        bool initializeSensors();
        bool finalizeMachineState();

        // Helper methods
        void calculateDerivedValues();
        void setupTiming();
        void registerMQTTParameters();
        void registerMQTTSensors();
};