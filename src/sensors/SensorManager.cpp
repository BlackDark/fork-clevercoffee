/**
 * @file SensorManager.cpp
 * @brief Implementation of RAII wrapper for sensor management
 */

#include "SensorManager.h"
#include "../Config.h"
#include "../hardware/pressureSensor.h"
#include "Logger.h"
#include <Arduino.h>
#include <Wire.h>

// Forward declarations for scale-related functions and variables
extern void initScale(); // This is defined in scaleHandler.h but used in main.cpp

// External global variables that need to be accessed (defined in scaleHandler.h as float)
extern float currBrewWeight;
extern float currReadingWeight;
extern bool scaleFailure;
extern Scale* scale;

SensorManager::SensorManager() :
    tempSensor_(nullptr),
    waterTankSensor_(nullptr),
    sensorsInitialized_(false),
    temperature_(0.0),
    brewTempOffset_(0.0),
    waterTankFull_(true),
    waterTankCheckConsecutiveReads_(0),
    inputPressure_(0.0f),
    inputPressureFilter_(0.0f),
    inX_(0.0f),
    inY_(0.0f),
    inOld_(0.0f),
    inSum_(0.0f),
    previousMillisPressure_(0) {
}

bool SensorManager::initialize(TempSensor* tempSensor, Switch* waterTankSensor) {
    LOG(INFO, "Initializing SensorManager");

    tempSensor_ = tempSensor;
    waterTankSensor_ = waterTankSensor;

    // Get brew temperature offset from config
    brewTempOffset_ = Config::getInstance().get<double>("brew.temp_offset");

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
        temperature_ = tempSensor_->getCurrentTemperature();
        temperature_ -= brewTempOffset_;
    }

    // Update water tank sensor (handled by timer in main.cpp)
    // updateWaterTankSensor(); - Called by timer

    // Update pressure sensor
    updatePressureSensor();

    // Update scale (handled in main loop)
    // updateScale(); - Called from main loop
}

bool SensorManager::areSensorsReady() const {
    // Check if any enabled sensor has an error
    if (Config::getInstance().get<bool>("hardware.sensors.temperature.enabled") && hasTemperatureError()) {
        return false;
    }

    // Water tank sensor doesn't have error state, just reports empty/full
    
    // Pressure sensor doesn't have explicit error state
    
    // Scale error is handled via global scaleFailure variable
    if (Config::getInstance().get<bool>("hardware.sensors.scale.enabled") && hasScaleError()) {
        return false;
    }

    return true;
}

bool SensorManager::hasSensorError() const {
    return !areSensorsReady();
}

double SensorManager::getCurrentTemperature() const {
    return temperature_;
}

bool SensorManager::hasTemperatureError() const {
    return tempSensor_ != nullptr && tempSensor_->hasError();
}

bool SensorManager::isWaterTankFull() const {
    return waterTankFull_;
}

void SensorManager::updateWaterTankSensor() {
    if (!Config::getInstance().get<bool>("hardware.sensors.watertank.enabled") || waterTankSensor_ == nullptr) {
        return;
    }

    if (const bool isWaterDetected = waterTankSensor_->isPressed(); isWaterDetected && !waterTankFull_) {
        waterTankFull_ = true;
        waterTankCheckConsecutiveReads_ = 0;
        LOG(INFO, "Water tank is full");
    }
    else if (!isWaterDetected && waterTankFull_) {
        waterTankFull_ = false;
        waterTankCheckConsecutiveReads_ = 0;
        LOG(INFO, "Water tank is empty");
    }
}

float SensorManager::getCurrentPressure() const {
    return inputPressure_;
}

float SensorManager::getFilteredPressure() const {
    return inputPressureFilter_;
}

void SensorManager::updatePressureSensor() {
    if (!Config::getInstance().get<bool>("hardware.sensors.pressure.enabled")) {
        return;
    }

    if (const unsigned long currentMillisPressure = millis(); 
        currentMillisPressure - previousMillisPressure_ >= intervalPressure_) {
        previousMillisPressure_ = currentMillisPressure;
        inputPressure_ = measurePressure();
        inputPressureFilter_ = filterPressureValue(inputPressure_);
    }
}

bool SensorManager::initializeScale() {
    if (!Config::getInstance().get<bool>("hardware.sensors.scale.enabled")) {
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
    if (!Config::getInstance().get<bool>("hardware.sensors.scale.enabled")) {
        return;
    }

    // Scale update is handled by global functions in main.cpp:
    // checkWeight() and shotTimerScale()
    // These are called from the main loop
}

float SensorManager::getCurrentWeight() const {
    return currReadingWeight;
}

float SensorManager::getCurrentBrewWeight() const {
    return currBrewWeight;
}

bool SensorManager::hasScaleError() const {
    return scaleFailure;
}

Scale* SensorManager::getScale() const {
    return scale;
}

bool SensorManager::initializeTemperatureSensor() {
    if (!Config::getInstance().get<bool>("hardware.sensors.temperature.enabled")) {
        LOG(INFO, "Temperature sensor disabled in configuration");
        return true;
    }

    if (tempSensor_ == nullptr) {
        LOG(ERROR, "Temperature sensor not provided to SensorManager");
        return false;
    }

    // Temperature sensor is already initialized by HardwareManager
    // Just get the initial reading
    temperature_ = tempSensor_->getCurrentTemperature();
    temperature_ -= brewTempOffset_;
    
    LOG(INFO, "Temperature sensor initialized via SensorManager");
    return true;
}

bool SensorManager::initializeWaterTankSensor() {
    if (!Config::getInstance().get<bool>("hardware.sensors.watertank.enabled")) {
        LOG(INFO, "Water tank sensor disabled in configuration");
        return true;
    }

    if (waterTankSensor_ == nullptr) {
        LOG(WARNING, "Water tank sensor not provided to SensorManager");
        return false;
    }

    // Initialize water tank state
    waterTankFull_ = true;
    waterTankCheckConsecutiveReads_ = 0;
    
    LOG(INFO, "Water tank sensor initialized via SensorManager");
    return true;
}

bool SensorManager::initializePressureSensor() {
    if (!Config::getInstance().get<bool>("hardware.sensors.pressure.enabled")) {
        LOG(INFO, "Pressure sensor disabled in configuration");
        return true;
    }

    // Initialize pressure sensor variables
    inputPressure_ = 0.0f;
    inputPressureFilter_ = 0.0f;
    inX_ = 0.0f;
    inY_ = 0.0f;
    inOld_ = 0.0f;
    inSum_ = 0.0f;
    previousMillisPressure_ = millis();

    LOG(INFO, "Pressure sensor initialized via SensorManager");
    return true;
}

float SensorManager::filterPressureValue(float input) {
    // Low-pass filter implementation (originally from main.cpp)
    // y(n) = 0.3 * x(n) + 0.7 * y(n-1)
    // multiplier must be 1 increase inX multiplier to make the filter faster
    inX_ = static_cast<float>(input * 0.3);
    inY_ = static_cast<float>(inOld_ * 0.7);
    inSum_ = inX_ + inY_;
    inOld_ = inSum_;

    return inSum_;
}

float SensorManager::measurePressure() {
    // Use the global measurePressure function from pressureSensor.h
    return ::measurePressure();
}