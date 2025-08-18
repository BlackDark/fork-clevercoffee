/**
 * @file SensorManager.cpp
 * @brief Implementation of RAII wrapper for sensor management
 */

#include "clevercoffee/sensors/SensorManager.h"

#include "clevercoffee/Config.h"
#include "clevercoffee/GlobalState.h"
#include "clevercoffee/Logger.h"
#include "clevercoffee/hardware/pressureSensor.h"

#include <Arduino.h>
#include <Wire.h>

// Scale handler functions
#include "clevercoffee/scaleHandler.h"

SensorManager::SensorManager()
    : tempSensor_(nullptr), waterTankSensor_(nullptr), sensorsInitialized_(false), temperature_(0.0),
      waterTankFull_(true), waterTankCheckConsecutiveReads_(0), inputPressure_(0.0f), inputPressureFilter_(0.0f),
      inX_(0.0f), inY_(0.0f), inOld_(0.0f), inSum_(0.0f), previousMillisPressure_(0), pressureHistory_{},
      tempHistory_{}, pressureHistoryIndex_(0), tempHistoryIndex_(0) {}

bool SensorManager::initialize(TempSensor* tempSensor, Switch* waterTankSensor) {
    LOG(INFO, "Initializing SensorManager");

    tempSensor_      = tempSensor;
    waterTankSensor_ = waterTankSensor;

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

    // Update temperature reading
    if (tempSensor_ != nullptr) {
        g_state.coordination.temperatureUpdateRunning = true;
        temperature_                                  = tempSensor_->getCurrentTemperature();
        g_state.coordination.temperatureUpdateRunning = false;
    }

    // Update water tank sensor (handled by timer in main.cpp)
    // updateWaterTankSensor(); - Called by timer

    // Update pressure sensor
    updatePressureSensor();

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

    if (const unsigned long currentMillisPressure = millis();
        currentMillisPressure - previousMillisPressure_ >= intervalPressure_) {
        previousMillisPressure_ = currentMillisPressure;
        inputPressure_          = measurePressure();
        inputPressureFilter_    = filterPressureValue(inputPressure_);
    }
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
    inputPressure_          = 0.0f;
    inputPressureFilter_    = 0.0f;
    inX_                    = 0.0f;
    inY_                    = 0.0f;
    inOld_                  = 0.0f;
    inSum_                  = 0.0f;
    previousMillisPressure_ = millis();

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
