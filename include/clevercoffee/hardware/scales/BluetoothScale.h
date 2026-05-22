/**
 * @file BluetoothScale.h
 * @brief Bluetooth scale implementation with ISensor support
 */

#pragma once

#include "clevercoffee/hardware/scales/Scale.h"
#include "clevercoffee/sensors/ISensor.h"

#include <AcaiaArduinoBLE.h>

using CleverCoffee::Error;
using CleverCoffee::Expected;
using CleverCoffee::ISensor;

/**
 * @brief Bluetooth scale implementation for Acaia and compatible scales
 * Implements both Scale (legacy) and ISensor (new) interfaces
 */
class BluetoothScale : public Scale, public ISensor {
  public:
    BluetoothScale();

    ~BluetoothScale() override;

    // Scale interface (legacy)
    bool                init() override;
    bool                update() override;
    [[nodiscard]] float getWeight() const noexcept override;
    void                tare() override;
    void                setSamples(int samples) override;
    [[nodiscard]] bool  isConnected() const noexcept override;

    // ISensor interface (new async pattern)
    void                    startRead() noexcept override;
    Expected<double, Error> tryGetValue() noexcept override;
    const char*             getSensorType() const noexcept override;
    void                    requestTare() noexcept override;

    void               updateConnection();
    [[nodiscard]] bool isConnecting() const;

  private:
    AcaiaArduinoBLE* bleScale;
    float            currentWeight;
    unsigned long    lastUpdateTime;
    bool             connected;

    // Connection retry mechanism
    bool          bleInitialized;
    unsigned long lastConnectionAttempt;
    unsigned long connectionAttemptInterval;

    bool          isUpdatingConnection;
    unsigned long maxConnectionAttemptInterval;

    // ISensor state tracking
    unsigned long                  lastSuccessfulRead_ = 0;
    static constexpr unsigned long READ_TIMEOUT_MS     = 1000; // Bluetooth may be slower
};
