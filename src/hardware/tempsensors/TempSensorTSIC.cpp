/**
 * @file TempSensorTSIC.cpp
 *
 * @brief Handler for TSIC 306 temperature sensor
 */

#include "clevercoffee/hardware/tempsensors/TempSensorTSIC.h"

#include "clevercoffee/Logger.h"

#define INITIAL_CHANGERATE 200
#define RUNTIME_CHANGERATE 5

TempSensorTSIC::TempSensorTSIC(const int GPIOPin) {
    // Set pin to receive signal from the TSic 306
    tsicSensor_ = new ZACwire(GPIOPin, 306);
    // Start sampling the TSic sensor
    tsicSensor_->begin();
}

TempSensorTSIC::~TempSensorTSIC() {
    if (tsicSensor_ != nullptr) {
        delete tsicSensor_;
        tsicSensor_ = nullptr;
    }
}

bool TempSensorTSIC::sample_temperature(double& temperature) const {
    static bool validTemps = false;
    float       temp       = 0.0;

    if (!validTemps) {
        temp = tsicSensor_->getTemp(INITIAL_CHANGERATE);

        // Once the current and previous readings are both in range and close together,
        // latch onto the tighter runtime change-rate so glitch smoothing becomes active.
        // `temperature` carries the previous good reading (seeded by the caller).
        if (temp > 0.0 && temp < 180.0 && temperature > 0.0 && temperature < 180.0 &&
            abs(temperature - temp) < RUNTIME_CHANGERATE) {
            validTemps = true;
        }
    } else {
        temp = tsicSensor_->getTemp(RUNTIME_CHANGERATE);
    }

    if (temp == 222) {
        LOG(WARNING, "Temperature reading failed");
        return false;
    }

    if (temp == 221) {
        LOG(WARNING, "Temperature sensor not connected");
        return false;
    }

    // Reject physically impossible readings (sensor glitches) instead of passing them
    // through as valid. A single spurious value (e.g. -2.9°C) must not trip emergency
    // stop; treating it as a failed read keeps the last good cached value instead.
    if (temp <= 0.0 || temp >= 180.0) {
        LOGF(WARNING, "Temperature reading out of range, ignoring: %.1f°C", temp);
        return false;
    }

    temperature = temp;

    return true;
}
