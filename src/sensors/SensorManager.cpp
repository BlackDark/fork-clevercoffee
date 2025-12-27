/**
 * @file SensorManager.cpp
 * @brief Implementation of RAII wrapper for sensor management
 */

#include "clevercoffee/sensors/SensorManager.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/hardware/pressureSensor.h"
#include "clevercoffee/coordinators/SensorCoordinator.h"

#include <Arduino.h>
#include <Wire.h>

// Scale handler functions
#include "clevercoffee/scaleHandler.h"

SensorManager::SensorManager()
    : tempSensor_(nullptr), waterTankSensor_(nullptr), coordinator_(nullptr), sensorsInitialized_(false), temperature_(0.0),
      waterTankFull_(true), waterTankCheckConsecutiveReads_(0), inputPressure_(0.0f), inputPressureFilter_(0.0f),
      inX_(0.0f), inY_(0.0f), inOld_(0.0f), inSum_(0.0f), pressureHistory_{},
      tempHistory_{}, pressureHistoryIndex_(0), tempHistoryIndex_(0) {}

bool SensorManager::initialize(TempSensor* tempSensor, Switch* waterTankSensor, CleverCoffee::SensorCoordinator* coord) {
    LOG(INFO, "Initializing SensorManager");

    tempSensor_      = tempSensor;
    waterTankSensor_ = waterTankSensor;
    coordinator_     = coord;

    bool success = true;

    // Initialize individual sensors
    if (!initializeTemperatureSensor()) {
        LOG(WARNING, "Temperature sensor initialization failed");
        success = false;
    }

    if (!initializeWaterTankSensor()) {
        LOG(WARNING, "Water tank sensor initialization failed");
        success = false;
    }

    if (!initializePressureSensor()) {
        LOG(WARNING, "Pressure sensor initialization failed");
        success = false;
    }

    // Note: Scale initialization is handled separately via initializeScale()
    // because it requires complex global dependencies

    sensorsInitialized_ = success;
    LOG(INFO, "SensorManager initialization completed");
    return success;
}

void SensorManager::update() {
    if (!sensorsInitialized_) {
        return;
    }

    // Update temperature reading with timeout protection and caching
    if (tempSensor_ != nullptr) {
        static unsigned long lastTempReadTime = 0;
        static unsigned long tempTimeoutCount = 0;
        static double cachedTemperature = 20.0; // Default fallback temperature
        
        const unsigned long currentTime = millis();
        
        // Skip sensor read if we've had recent timeouts - start skipping after 2 timeouts
        bool shouldSkipRead = (tempTimeoutCount >= 2);
        
        if (!shouldSkipRead) {
            if (coordinator_) {
                coordinator_->startTemperatureUpdate();
            }

            // Add timeout protection for sensor reading
            const unsigned long startTime = millis();
            const bool updateSuccess = tempSensor_->updateTemperature();
            const unsigned long readTime = millis() - startTime;

            if (readTime > 500) {
                tempTimeoutCount++;
                LOGF(WARNING, "Temperature sensor took %lums to read (timeout #%lu) - switching to cached mode",
                     readTime, tempTimeoutCount);
                temperature_ = cachedTemperature; // Use cached value
            } else if (updateSuccess) {
                // Successful read - get the updated temperature and reset timeout counter
                const double newTemp = tempSensor_->getCurrentTemperature();
                temperature_ = newTemp;
                cachedTemperature = newTemp;
                if (tempTimeoutCount > 0) {
                    LOG(INFO, "Temperature sensor recovered - resuming normal reads");
                    tempTimeoutCount = 0;
                }
            } else {
                // Update failed, use cached temperature
                LOGF(WARNING, "Temperature sensor update failed - using cached temperature: %.1f°C", cachedTemperature);
                temperature_ = cachedTemperature;
            }

            if (coordinator_) {
                coordinator_->stopTemperatureUpdate();
            }
        } else {
            // Skip this read completely - no sensor call at all
            temperature_ = cachedTemperature;
            
            static unsigned long lastSkipLog = 0;
            if (currentTime - lastSkipLog > 10000) { // Log every 10 seconds
                LOGF(INFO, "Skipping temperature reads due to sensor timeouts - using cached %.1f°C", cachedTemperature);
                lastSkipLog = currentTime;
            }
        }
    }

    // Update water tank sensor (handled by timer in main.cpp)
    // updateWaterTankSensor(); - Called by timer

    // Update pressure sensor (handled by LoopManager timer)
    // updatePressureSensor(); - Called separately by LoopManager

    // Update scale (handled in main loop)
    // updateScale(); - Called from main loop
}

bool SensorManager::areSensorsReady() const noexcept {
    // Check if any enabled sensor has an error
    // Temperature sensor is always enabled, just check for errors
    if (hasTemperatureError()) {
        return false;
    }

    // Water tank sensor doesn't have error state, just reports empty/full

    // Pressure sensor doesn't have explicit error state

    // Scale error is handled via global scaleFailure variable
    if (Config::getInstance().hardwareSensorsScaleEnabled.get() && hasScaleError()) {
        return false;
    }

    return true;
}

bool SensorManager::hasSensorError() const noexcept {
    return !areSensorsReady();
}

double SensorManager::getCurrentTemperature() const noexcept {
    return temperature_;
}

bool SensorManager::hasTemperatureError() const noexcept {
    return tempSensor_ != nullptr && tempSensor_->hasError();
}

bool SensorManager::isWaterTankFull() const noexcept {
    return waterTankFull_;
}

void SensorManager::updateWaterTankSensor() {
    if (!Config::getInstance().hardwareSensorsWatertankEnabled.get() || waterTankSensor_ == nullptr) {
        return;
    }

    if (const bool isWaterDetected = waterTankSensor_->isPressed(); isWaterDetected && !waterTankFull_) {
        waterTankFull_                  = true;
        waterTankCheckConsecutiveReads_ = 0;
        LOG(INFO, "Water tank is full");
    } else if (!isWaterDetected && waterTankFull_) {
        waterTankFull_                  = false;
        waterTankCheckConsecutiveReads_ = 0;
        LOG(INFO, "Water tank is empty");
    }
}

float SensorManager::getCurrentPressure() const noexcept {
    return inputPressure_;
}

float SensorManager::getFilteredPressure() const noexcept {
    return inputPressureFilter_;
}

void SensorManager::updatePressureSensor() {
    if (!Config::getInstance().hardwareSensorsPressureEnabled.get()) {
        return;
    }

    static unsigned long pressureTimeoutCount = 0;
    static float cachedPressure = 0.0f; // Default fallback pressure

    // Remove internal timing - let LoopManager control when this is called
    // Skip sensor read if we've had timeouts - start skipping after 2 timeouts  
    bool shouldSkipRead = (pressureTimeoutCount >= 2);
    
    if (!shouldSkipRead) {
        // Add timeout protection for pressure sensor reading
        const unsigned long startTime = millis();
        const float newPressure = measurePressure();
        const unsigned long readTime = millis() - startTime;
        
        if (readTime > 100) {
            pressureTimeoutCount++;
            LOGF(WARNING, "Pressure sensor took %lums to read (timeout #%lu) - switching to cached mode", 
                 readTime, pressureTimeoutCount);
            inputPressure_ = cachedPressure; // Use cached value
        } else {
            // Successful read - update cache and reset timeout counter
            inputPressure_ = newPressure;
            cachedPressure = newPressure;
            if (pressureTimeoutCount > 0) {
                LOG(INFO, "Pressure sensor recovered - resuming normal reads");
                pressureTimeoutCount = 0;
            }
        }
    } else {
        // Skip this read completely - no sensor call at all
        inputPressure_ = cachedPressure;
        
        static unsigned long lastSkipLog = 0;
        const unsigned long currentTime = millis();
        if (currentTime - lastSkipLog > 10000) { // Log every 10 seconds
            LOGF(INFO, "Skipping pressure reads due to sensor timeouts - using cached %.2f bar", cachedPressure);
            lastSkipLog = currentTime;
        }
    }
    
    inputPressureFilter_ = filterPressureValue(inputPressure_);
}

bool SensorManager::initializeScale() {
    if (!Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        LOG(INFO, "Scale sensor disabled in configuration");
        return true;
    }

    try {
        // Call the global initScale function from scaleHandler.h
        initScale();
        LOG(INFO, "Scale sensor initialized via SensorManager");
        return true;
    } catch (const std::exception& e) {
        LOGF(ERROR, "Scale initialization failed: %s", e.what());
        return false;
    }
}

void SensorManager::updateScale() {
    if (!Config::getInstance().hardwareSensorsScaleEnabled.get()) {
        return;
    }

    // TODO
    // Scale update is handled by global functions in main.cpp:
    // checkWeight() and shotTimerScale()
    // These are called from the main loop
}

float SensorManager::getCurrentWeight() const noexcept {
    return g_state.sensors.currReadingWeight;
}

float SensorManager::getCurrentBrewWeight() const noexcept {
    return g_state.sensors.currBrewWeight;
}

bool SensorManager::hasScaleError() const noexcept {
    return g_state.sensors.scaleFailure;
}

Scale* SensorManager::getScale() const noexcept {
    return g_state.hardware.scale.get();
}

bool SensorManager::initializeTemperatureSensor() {
    // Temperature sensor is always enabled - check if we have a sensor instance

    if (tempSensor_ == nullptr) {
        LOG(ERROR, "Temperature sensor not provided to SensorManager");
        return false;
    }

    // Temperature sensor is already initialized by HardwareManager
    // Just get the initial reading
    temperature_ = tempSensor_->getCurrentTemperature();

    LOG(INFO, "Temperature sensor initialized via SensorManager");
    return true;
}

bool SensorManager::initializeWaterTankSensor() {
    if (!Config::getInstance().hardwareSensorsWatertankEnabled.get()) {
        LOG(INFO, "Water tank sensor disabled in configuration");
        return true;
    }

    if (waterTankSensor_ == nullptr) {
        LOG(WARNING, "Water tank sensor not provided to SensorManager");
        return false;
    }

    // Initialize water tank state
    waterTankFull_                  = true;
    waterTankCheckConsecutiveReads_ = 0;

    LOG(INFO, "Water tank sensor initialized via SensorManager");
    return true;
}

bool SensorManager::initializePressureSensor() {
    if (!Config::getInstance().hardwareSensorsPressureEnabled.get()) {
        LOG(INFO, "Pressure sensor disabled in configuration");
        return true;
    }

    // Initialize pressure sensor variables
    inputPressure_       = 0.0f;
    inputPressureFilter_ = 0.0f;
    inX_                 = 0.0f;
    inY_                 = 0.0f;
    inOld_               = 0.0f;
    inSum_               = 0.0f;

    LOG(INFO, "Pressure sensor initialized via SensorManager");
    return true;
}

float SensorManager::filterPressureValue(float input) {
    // Low-pass filter implementation (originally from main.cpp)
    // y(n) = 0.3 * x(n) + 0.7 * y(n-1)
    // multiplier must be 1 increase inX multiplier to make the filter faster
    inX_   = static_cast<float>(input * 0.3);
    inY_   = static_cast<float>(inOld_ * 0.7);
    inSum_ = inX_ + inY_;
    inOld_ = inSum_;

    return inSum_;
}

float SensorManager::measurePressure() {
    // Use the global measurePressure function from pressureSensor.h
    return ::measurePressure();
}

// Modern sensor processing implementations using STL algorithms

double SensorManager::processTemperatureReadings(const std::vector<double>& readings) const {
    if (readings.empty()) {
        LOG(WARNING, "No temperature readings provided");
        return 0.0;
    }

    // Filter valid temperature readings (0-200°C range)
    std::vector<double> validReadings;
    std::copy_if(readings.begin(), readings.end(), std::back_inserter(validReadings), [](double temp) {
        return temp > 0.0 && temp < 200.0;
    });

    if (validReadings.empty()) {
        LOG(WARNING, "No valid temperature readings available");
        return 0.0;
    }

    // Calculate average of valid readings
    const double sum     = std::accumulate(validReadings.begin(), validReadings.end(), 0.0);
    const double average = sum / static_cast<double>(validReadings.size());

    // Apply outlier detection: remove readings more than 5°C from average
    std::vector<double> filteredReadings;
    std::copy_if(validReadings.begin(),
                 validReadings.end(),
                 std::back_inserter(filteredReadings),
                 [average](double temp) { return std::abs(temp - average) <= 5.0; });

    if (filteredReadings.empty()) {
        LOG(WARNING, "All temperature readings filtered out as outliers");
        return average;
    }

    // Final average after outlier removal
    const double filteredSum = std::accumulate(filteredReadings.begin(), filteredReadings.end(), 0.0);
    return filteredSum / static_cast<double>(filteredReadings.size());
}

float SensorManager::processPressureReadings(const std::array<float, 10>& readings) const {
    // Filter valid pressure readings (0-15 bar range for espresso machines)
    std::vector<float> validReadings;
    std::copy_if(readings.begin(), readings.end(), std::back_inserter(validReadings), [](float pressure) {
        return pressure >= 0.0f && pressure <= 15.0f;
    });

    if (validReadings.empty()) {
        LOG(WARNING, "No valid pressure readings available");
        return 0.0f;
    }

    // Sort readings to easily identify median and quartiles
    std::vector<float> sortedReadings = validReadings;
    std::sort(sortedReadings.begin(), sortedReadings.end());

    // Use interquartile range to filter outliers
    const size_t size = sortedReadings.size();
    if (size >= 4) {
        const size_t q1_idx     = size / 4;
        const size_t q3_idx     = (3 * size) / 4;
        const float  q1         = sortedReadings[q1_idx];
        const float  q3         = sortedReadings[q3_idx];
        const float  iqr        = q3 - q1;
        const float  lowerBound = q1 - 1.5f * iqr;
        const float  upperBound = q3 + 1.5f * iqr;

        // Filter out statistical outliers
        std::vector<float> outlierFilteredReadings;
        std::copy_if(sortedReadings.begin(),
                     sortedReadings.end(),
                     std::back_inserter(outlierFilteredReadings),
                     [lowerBound, upperBound](float p) { return p >= lowerBound && p <= upperBound; });

        if (!outlierFilteredReadings.empty()) {
            // Calculate weighted average with more weight on recent readings
            float weightedSum = 0.0f;
            float totalWeight = 0.0f;

            for (size_t i = 0; i < outlierFilteredReadings.size(); ++i) {
                const float weight  = 1.0f + (static_cast<float>(i) * 0.1f); // Recent readings get higher weight
                weightedSum        += outlierFilteredReadings[i] * weight;
                totalWeight        += weight;
            }

            return weightedSum / totalWeight;
        }
    }

    // Fallback: simple average of valid readings
    const float sum = std::accumulate(validReadings.begin(), validReadings.end(), 0.0f);
    return sum / static_cast<float>(validReadings.size());
}
