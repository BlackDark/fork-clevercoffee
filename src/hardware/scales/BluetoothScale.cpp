/**
 * @file BluetoothScale.cpp
 * @brief Bluetooth scale implementation with ISensor support
 */

#include "clevercoffee/hardware/scales/BluetoothScale.h"
#include "clevercoffee/errors/ErrorCodes.h"

#include "clevercoffee/Logger.h"

#include <Arduino.h>

using CleverCoffee::Expected;
using CleverCoffee::Error;
using CleverCoffee::ErrorCode;

BluetoothScale::BluetoothScale()
    : currentWeight(0.0), lastUpdateTime(0), connected(false), bleInitialized(false), lastConnectionAttempt(0),
      connectionAttemptInterval(5000), isUpdatingConnection(false), maxConnectionAttemptInterval(30000) {
    bleScale = new AcaiaArduinoBLE(false);
}

BluetoothScale::~BluetoothScale() {
    delete bleScale;
}

bool BluetoothScale::init() {
    LOG(INFO, "Starting Bluetooth scale initialization");

    const bool success = bleScale->init();

    if (success) {
        bleInitialized        = true;
        lastConnectionAttempt = millis();
        LOG(INFO, "BLE Scale initialization successful");
    } else {
        LOG(ERROR, "BLE Scale initialization failed");
        bleInitialized = false;
    }

    return success;
}

void BluetoothScale::updateConnection() {
    if (!bleInitialized) {
        return;
    }

    const unsigned long currentTime = millis();

    const bool wasConnecting = bleScale->isConnecting();
    bleScale->updateConnection();

    // Only update timing if we're not in a connection process or if connection just started
    if (!wasConnecting || bleScale->isConnecting()) {
        lastConnectionAttempt = currentTime;
    }

    // Check for connection state changes
    if (const bool newConnected = bleScale->isConnected(); newConnected != connected) {
        connected = newConnected;

        if (connected) {
            LOG(INFO, "Bluetooth scale connected");
            // Reset connection attempt interval on successful connection
            connectionAttemptInterval = 5000;
        } else {
            LOG(INFO, "Bluetooth scale disconnected");
            // Only increase interval if we're not actively connecting
            if (!bleScale->isConnecting()) {
                connectionAttemptInterval = min(connectionAttemptInterval * 2, 30000UL);
            }
        }
    }

    // If connection failed and we're not connecting, wait before retry
    if (!connected && !bleScale->isConnecting()) {
        if (currentTime - lastConnectionAttempt < connectionAttemptInterval) {
            return;
        }

        // Restart connection process by calling init again
        bleScale->init();
    }
}

bool BluetoothScale::isConnecting() const {
    if (!bleInitialized) {
        return false;
    }

    return bleScale->isConnecting();
}

bool BluetoothScale::update() {
    if (!bleInitialized) {
        return false;
    }

    if (connected) {
        if (bleScale->heartbeatRequired()) {
            bleScale->heartbeat();
        }

        if (bleScale->newWeightAvailable()) {
            // Allow negative values (post-tare) but filter out clearly invalid readings
            if (const float newWeight = bleScale->getWeight(); newWeight > -1000.0f && newWeight < 10000.0f) {
                currentWeight = newWeight;
                return true;
            }
        }
    }

    return false;
}

float BluetoothScale::getWeight() const noexcept {
    return currentWeight;
}

void BluetoothScale::tare() {
    if (connected) {
        bleScale->tare();
    }
}

void BluetoothScale::setSamples(int samples) {
    // Most BLE scales handle sampling internally
}

bool BluetoothScale::isConnected() const noexcept {
    return connected;
}

// ====== ISensor Interface Implementation ======

void BluetoothScale::startRead() noexcept {
    // Bluetooth scale continuously sends data when connected
    // This is a no-op for Bluetooth scales
}

Expected<double, Error> BluetoothScale::tryGetValue() noexcept {
    // Check if scale is connected
    if (!connected) {
        return Error(ErrorCode::SENSOR_DISCONNECTED, "Bluetooth scale not connected");
    }

    // Try to update (non-blocking)
    bool updated = update();

    if (updated) {
        lastSuccessfulRead_ = millis();
        return static_cast<double>(currentWeight);
    }

    // Check timeout
    if (millis() - lastSuccessfulRead_ > READ_TIMEOUT_MS) {
        return Error(ErrorCode::SENSOR_TIMEOUT, "Bluetooth scale read timeout");
    }

    // No new data yet, but not timed out
    return Error(ErrorCode::SENSOR_NOT_READY, "No new data yet");
}

const char* BluetoothScale::getSensorType() const noexcept {
    return "BluetoothScale (Acaia/Compatible)";
}
