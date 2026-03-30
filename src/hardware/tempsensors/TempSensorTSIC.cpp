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

        // if current and previous reading is in range and their difference is low then reduce changerate
        if (temp > 0.0 && temp < 180.0) {
            if (temperature > 0.0 && temperature < 180.0 && abs(temperature - temp) < RUNTIME_CHANGERATE) {
                validTemps = true;
            } else {
                LOGF(WARNING, "Temperature not stable");
            }
        } else if (temp != 221 && temp != 222) {
            LOGF(WARNING, "Temperature reading not within 0 - 180°C range: %0.01f°C", temp);
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

    temperature = temp;

    return true;
}
