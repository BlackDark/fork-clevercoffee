#pragma once

#include <atomic>

namespace CleverCoffee {

/**
 * @brief Coordinates network operations
 *
 * This class provides thread-safe tracking of network connection states,
 * replacing the previous global g_state.network flags. It manages WiFi and
 * MQTT connection status and tracks connection attempts for retry logic.
 *
 * The coordinator uses atomic operations to ensure thread safety when multiple
 * contexts may access network state simultaneously.
 *
 * @note This class is typically accessed through SystemContext::networkCoordinator()
 *
 * Example usage:
 * @code
 * SystemContext& ctx = ...;
 * ctx.networkCoordinator().setWifiConnected(true);
 *
 * if (ctx.networkCoordinator().isWifiConnected()) {
 *     ctx.networkCoordinator().setMqttConnected(true);
 *     ctx.networkCoordinator().resetMqttConnectionAttempts();
 * } else {
 *     int attempts = ctx.networkCoordinator().getMqttConnectionAttempts();
 *     if (attempts > MAX_RETRIES) {
 *         // Handle connection failure
 *     }
 * }
 * @endcode
 */
class NetworkCoordinator {
public:
    NetworkCoordinator() = default;

    /**
     * @name MQTT Connection State
     * @{
     */

    /**
     * @brief Set MQTT connection status
     *
     * Updates the MQTT connection state. Should be called when connection
     * is established or lost.
     *
     * @param connected true if MQTT is connected, false otherwise
     */
    void setMqttConnected(bool connected) noexcept {
        mqttConnected_ = connected;
    }

    /**
     * @brief Check if MQTT is connected
     *
     * @return true if MQTT connection is active, false otherwise
     */
    bool isMqttConnected() const noexcept {
        return mqttConnected_;
    }

    /** @} */

    /**
     * @name WiFi Connection State
     * @{
     */

    /**
     * @brief Set WiFi connection status
     *
     * Updates the WiFi connection state. Should be called when connection
     * is established or lost.
     *
     * @param connected true if WiFi is connected, false otherwise
     */
    void setWifiConnected(bool connected) noexcept {
        wifiConnected_ = connected;
    }

    /**
     * @brief Check if WiFi is connected
     *
     * @return true if WiFi connection is active, false otherwise
     */
    bool isWifiConnected() const noexcept {
        return wifiConnected_;
    }

    /** @} */

    /**
     * @name Connection Attempt Tracking
     * @{
     */

    /**
     * @brief Increment MQTT connection attempt counter
     *
     * Should be called before each MQTT connection attempt.
     *
     * @post getMqttConnectionAttempts() returns previous value + 1
     */
    void incrementMqttConnectionAttempts() noexcept {
        mqttConnectionAttempts_++;
    }

    /**
     * @brief Reset MQTT connection attempt counter
     *
     * Should be called when a connection is successfully established.
     *
     * @post getMqttConnectionAttempts() returns 0
     */
    void resetMqttConnectionAttempts() noexcept {
        mqttConnectionAttempts_ = 0;
    }

    /**
     * @brief Get current MQTT connection attempt count
     *
     * @return Number of connection attempts since last successful connection
     */
    int getMqttConnectionAttempts() const noexcept {
        return mqttConnectionAttempts_;
    }

    /** @} */

private:
    std::atomic<bool> mqttConnected_{false};          ///< MQTT connection state
    std::atomic<bool> wifiConnected_{false};          ///< WiFi connection state
    std::atomic<int> mqttConnectionAttempts_{0};      ///< MQTT retry counter
};

} // namespace CleverCoffee
