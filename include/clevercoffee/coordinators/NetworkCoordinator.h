#pragma once

#include <atomic>

namespace CleverCoffee {

/**
 * @brief Coordinates network operations
 *
 * Replaces g_state.network flags with thread-safe coordinator.
 * Tracks WiFi and MQTT connection state.
 */
class NetworkCoordinator {
public:
    NetworkCoordinator() = default;

    // MQTT connection state
    void setMqttConnected(bool connected) noexcept {
        mqttConnected_ = connected;
    }
    bool isMqttConnected() const noexcept {
        return mqttConnected_;
    }

    // WiFi connection state
    void setWifiConnected(bool connected) noexcept {
        wifiConnected_ = connected;
    }
    bool isWifiConnected() const noexcept {
        return wifiConnected_;
    }

    // Connection attempt tracking
    void incrementMqttConnectionAttempts() noexcept {
        mqttConnectionAttempts_++;
    }
    void resetMqttConnectionAttempts() noexcept {
        mqttConnectionAttempts_ = 0;
    }
    int getMqttConnectionAttempts() const noexcept {
        return mqttConnectionAttempts_;
    }

private:
    std::atomic<bool> mqttConnected_{false};
    std::atomic<bool> wifiConnected_{false};
    std::atomic<int> mqttConnectionAttempts_{0};
};

} // namespace CleverCoffee
