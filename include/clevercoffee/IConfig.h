/**
 * @file IConfig.h
 * @brief Abstract configuration interface for dependency injection and testing
 *
 * This interface provides virtual getters for the most commonly used configuration
 * parameters. It enables dependency injection in handlers, coordinators, and
 * controllers, replacing direct Config::getInstance() calls.
 *
 * Design:
 * - Pure virtual getters only (no setters — config mutation goes through Config)
 * - Covers the ~35 most frequently accessed parameters
 * - The real Config class will implement this in a future phase via adapter/bridge
 * - MockConfig implements this directly for unit testing
 *
 * @note The existing Config class does NOT inherit from IConfig yet. That bridge
 *       will be introduced in Phase 5 to avoid risk during the test infrastructure phase.
 */

#pragma once

#include "clevercoffee/defaults.h"

/**
 * @class IConfig
 * @brief Abstract interface for read-only configuration access
 *
 * Provides a testable abstraction over the Config singleton. Production code
 * can be refactored to accept `const IConfig&` instead of calling
 * `Config::getInstance()` directly, enabling mock injection in tests.
 */
class IConfig {
  public:
    virtual ~IConfig() = default;

    // ── PID Parameters ──────────────────────────────────────────────────

    /** @brief Whether PID controller is enabled */
    virtual bool getPidEnabled() const noexcept = 0;

    /** @brief Whether Proportional-on-Measurement mode is active */
    virtual bool getPidUsePonm() const noexcept = 0;

    /** @brief PID proportional gain (Kp) for regular mode */
    virtual double getPidRegularKp() const noexcept = 0;

    /** @brief PID integral time constant (Tn) for regular mode */
    virtual double getPidRegularTn() const noexcept = 0;

    /** @brief PID derivative time constant (Tv) for regular mode */
    virtual double getPidRegularTv() const noexcept = 0;

    /** @brief PID integrator maximum (anti-windup) for regular mode */
    virtual double getPidRegularIMax() const noexcept = 0;

    /** @brief PID Kp for steam mode */
    virtual double getPidSteamKp() const noexcept = 0;

    /** @brief PID EMA smoothing factor */
    virtual double getPidEmaFactor() const noexcept = 0;

    /** @brief Whether brew-detection PID is enabled */
    virtual bool getPidBdEnabled() const noexcept = 0;

    /** @brief Brew-detection PID Kp */
    virtual double getPidBdKp() const noexcept = 0;

    /** @brief Brew-detection PID Tn */
    virtual double getPidBdTn() const noexcept = 0;

    /** @brief Brew-detection PID Tv */
    virtual double getPidBdTv() const noexcept = 0;

    /** @brief Delay (seconds) before enabling PID during brew */
    virtual double getBrewPidDelay() const noexcept = 0;

    // ── Temperature / Setpoints ─────────────────────────────────────────

    /** @brief Brew temperature setpoint (°C) */
    virtual double getBrewSetpoint() const noexcept = 0;

    /** @brief Brew temperature offset (°C) */
    virtual double getBrewTempOffset() const noexcept = 0;

    /** @brief Steam temperature setpoint (°C) */
    virtual double getSteamSetpoint() const noexcept = 0;

    /** @brief Emergency stop temperature threshold (°C) */
    virtual double getEmergencyStopTemp() const noexcept = 0;

    /** @brief Emergency stop hysteresis (°C) */
    virtual double getEmergencyStopHysteresis() const noexcept = 0;

    // ── Brew Control ────────────────────────────────────────────────────

    /** @brief Whether brew-by-time is enabled */
    virtual bool getBrewByTimeEnabled() const noexcept = 0;

    /** @brief Target brew time (seconds) */
    virtual double getBrewByTimeTargetTime() const noexcept = 0;

    /** @brief Whether brew-by-weight is enabled */
    virtual bool getBrewByWeightEnabled() const noexcept = 0;

    /** @brief Target brew weight (grams) */
    virtual double getBrewByWeightTargetWeight() const noexcept = 0;

    /** @brief Whether pre-infusion is enabled */
    virtual bool getBrewPreInfusionEnabled() const noexcept = 0;

    /** @brief Pre-infusion time (seconds) */
    virtual double getBrewPreInfusionTime() const noexcept = 0;

    /** @brief Pre-infusion pause time (seconds) */
    virtual double getBrewPreInfusionPause() const noexcept = 0;

    /** @brief Current brew mode */
    virtual Process::BrewMode getBrewMode() const noexcept = 0;

    // ── Hardware Switches ───────────────────────────────────────────────

    /** @brief Whether brew switch is enabled */
    virtual bool getHardwareSwitchesBrewEnabled() const noexcept = 0;

    /** @brief Brew switch type (TOGGLE / MOMENTARY) */
    virtual Hardware::SwitchType getHardwareSwitchesBrewType() const noexcept = 0;

    /** @brief Whether steam switch is enabled */
    virtual bool getHardwareSwitchesSteamEnabled() const noexcept = 0;

    /** @brief Steam switch type */
    virtual Hardware::SwitchType getHardwareSwitchesSteamType() const noexcept = 0;

    /** @brief Whether power switch is enabled */
    virtual bool getHardwareSwitchesPowerEnabled() const noexcept = 0;

    /** @brief Power switch type */
    virtual Hardware::SwitchType getHardwareSwitchesPowerType() const noexcept = 0;

    /** @brief Whether hot water switch is enabled */
    virtual bool getHardwareSwitchesHotWaterEnabled() const noexcept = 0;

    /** @brief Hot water switch type */
    virtual Hardware::SwitchType getHardwareSwitchesHotWaterType() const noexcept = 0;

    // ── Hardware Sensors & Peripherals ──────────────────────────────────

    /** @brief Whether scale is enabled */
    virtual bool getHardwareSensorsScaleEnabled() const noexcept = 0;

    /** @brief Whether pressure sensor is enabled */
    virtual bool getHardwareSensorsPressureEnabled() const noexcept = 0;

    /** @brief Whether OLED display is enabled */
    virtual bool getHardwareOledEnabled() const noexcept = 0;

    // ── Hardware LEDs ───────────────────────────────────────────────────

    /** @brief Whether status LED is enabled */
    virtual bool getHardwareLedsStatusEnabled() const noexcept = 0;

    /** @brief Whether brew LED is enabled */
    virtual bool getHardwareLedsBrewEnabled() const noexcept = 0;

    /** @brief Whether steam LED is enabled */
    virtual bool getHardwareLedsSteamEnabled() const noexcept = 0;

    // ── Standby ─────────────────────────────────────────────────────────

    /** @brief Whether standby timer is enabled */
    virtual bool getStandbyEnabled() const noexcept = 0;

    /** @brief Standby time (minutes) */
    virtual double getStandbyTime() const noexcept = 0;

    // ── Backflush ───────────────────────────────────────────────────────

    /** @brief Number of backflush cycles */
    virtual int getBackflushCycles() const noexcept = 0;

    /** @brief Backflush fill time (seconds) */
    virtual double getBackflushFillTime() const noexcept = 0;

    /** @brief Backflush flush time (seconds) */
    virtual double getBackflushFlushTime() const noexcept = 0;

    // ── MQTT ────────────────────────────────────────────────────────────

    /** @brief Whether MQTT is enabled */
    virtual bool getMqttEnabled() const noexcept = 0;

    // ── Display ─────────────────────────────────────────────────────────

    /** @brief Display template selection */
    virtual System::DisplayTemplate getDisplayTemplate() const noexcept = 0;

    /** @brief Display language */
    virtual System::Language getDisplayLanguage() const noexcept = 0;
};
