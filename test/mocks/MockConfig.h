/**
 * @file MockConfig.h
 * @brief Mock implementation of IConfig for testing
 *
 * Implements the IConfig interface with simple in-memory storage.
 * All values are directly settable via public members or setter methods,
 * making it easy to configure test scenarios.
 *
 * Usage:
 * @code
 * MockConfig mockConfig;
 * mockConfig.setPidEnabled(true);
 * mockConfig.setBrewSetpoint(95.0);
 *
 * // Pass to code under test
 * ProcessController controller(mockConfig, ...);
 * @endcode
 */

#pragma once

#include "clevercoffee/IConfig.h"
#include "clevercoffee/defaults.h"

#include <Arduino.h>

class MockConfig : public IConfig {
  public:
    MockConfig()  = default;
    ~MockConfig() = default;

    // ── PID Parameters ──────────────────────────────────────────────

    bool   pidEnabled_     = true;
    bool   pidUsePonm_     = false;
    double pidRegularKp_   = AGGKP;
    double pidRegularTn_   = AGGTN;
    double pidRegularTv_   = AGGTV;
    double pidRegularIMax_ = AGGIMAX;
    double pidSteamKp_     = STEAMKP;
    double pidEmaFactor_   = EMA_FACTOR;
    bool   pidBdEnabled_   = false;
    double pidBdKp_        = AGGBKP;
    double pidBdTn_        = AGGBTN;
    double pidBdTv_        = AGGBTV;
    double brewPidDelay_   = BREW_PID_DELAY;

    // ── Temperature / Setpoints ─────────────────────────────────────

    double brewSetpoint_            = SETPOINT;
    double brewTempOffset_          = TEMPOFFSET;
    double steamSetpoint_           = STEAMSETPOINT;
    double emergencyStopTemp_       = 150.0;
    double emergencyStopHysteresis_ = 5.0;

    // ── Brew Control ────────────────────────────────────────────────

    bool              brewByTimeEnabled_        = false;
    double            brewByTimeTargetTime_     = TARGET_BREW_TIME;
    bool              brewByWeightEnabled_      = false;
    double            brewByWeightTargetWeight_ = TARGET_BREW_WEIGHT;
    bool              brewPreInfusionEnabled_   = false;
    double            brewPreInfusionTime_      = PRE_INFUSION_TIME;
    double            brewPreInfusionPause_     = PRE_INFUSION_PAUSE_TIME;
    Process::BrewMode brewMode_                 = Process::BrewMode::MANUAL_BREW;

    // ── Hardware Switches ───────────────────────────────────────────

    bool                 hardwareSwitchesBrewEnabled_     = true;
    Hardware::SwitchType hardwareSwitchesBrewType_        = Hardware::SwitchType::MOMENTARY;
    bool                 hardwareSwitchesSteamEnabled_    = true;
    Hardware::SwitchType hardwareSwitchesSteamType_       = Hardware::SwitchType::MOMENTARY;
    bool                 hardwareSwitchesPowerEnabled_    = false;
    Hardware::SwitchType hardwareSwitchesPowerType_       = Hardware::SwitchType::TOGGLE;
    bool                 hardwareSwitchesHotWaterEnabled_ = true;
    Hardware::SwitchType hardwareSwitchesHotWaterType_    = Hardware::SwitchType::MOMENTARY;

    // ── Hardware Sensors & Peripherals ──────────────────────────────

    bool hardwareSensorsScaleEnabled_    = false;
    bool hardwareSensorsPressureEnabled_ = false;
    bool hardwareOledEnabled_            = true;

    // ── Hardware LEDs ───────────────────────────────────────────────

    bool hardwareLedsStatusEnabled_ = false;
    bool hardwareLedsBrewEnabled_   = false;
    bool hardwareLedsSteamEnabled_  = false;

    // ── Standby ─────────────────────────────────────────────────────

    bool   standbyEnabled_ = true;
    double standbyTime_    = STANDBY_MODE_TIME;

    // ── Backflush ───────────────────────────────────────────────────

    int    backflushCycles_    = BACKFLUSH_CYCLES;
    double backflushFillTime_  = BACKFLUSH_FILL_TIME;
    double backflushFlushTime_ = BACKFLUSH_FLUSH_TIME;

    // ── MQTT ────────────────────────────────────────────────────────

    bool mqttEnabled_ = false;

    // ── Display ─────────────────────────────────────────────────────

    System::DisplayTemplate displayTemplate_ = System::DisplayTemplate::STANDARD;
    System::Language        displayLanguage_  = System::Language::ENGLISH;

    // ── Auth (not in IConfig but useful for some tests) ─────────────

    bool   systemAuthEnabled_  = false;
    String systemAuthUsername_  = "";
    String systemAuthPassword_  = "";

    // ═══════════════════════════════════════════════════════════════
    // IConfig virtual getters
    // ═══════════════════════════════════════════════════════════════

    // PID
    bool   getPidEnabled() const noexcept override { return pidEnabled_; }
    bool   getPidUsePonm() const noexcept override { return pidUsePonm_; }
    double getPidRegularKp() const noexcept override { return pidRegularKp_; }
    double getPidRegularTn() const noexcept override { return pidRegularTn_; }
    double getPidRegularTv() const noexcept override { return pidRegularTv_; }
    double getPidRegularIMax() const noexcept override { return pidRegularIMax_; }
    double getPidSteamKp() const noexcept override { return pidSteamKp_; }
    double getPidEmaFactor() const noexcept override { return pidEmaFactor_; }
    bool   getPidBdEnabled() const noexcept override { return pidBdEnabled_; }
    double getPidBdKp() const noexcept override { return pidBdKp_; }
    double getPidBdTn() const noexcept override { return pidBdTn_; }
    double getPidBdTv() const noexcept override { return pidBdTv_; }
    double getBrewPidDelay() const noexcept override { return brewPidDelay_; }

    // Temperature / Setpoints
    double getBrewSetpoint() const noexcept override { return brewSetpoint_; }
    double getBrewTempOffset() const noexcept override { return brewTempOffset_; }
    double getSteamSetpoint() const noexcept override { return steamSetpoint_; }
    double getEmergencyStopTemp() const noexcept override { return emergencyStopTemp_; }
    double getEmergencyStopHysteresis() const noexcept override { return emergencyStopHysteresis_; }

    // Brew Control
    bool              getBrewByTimeEnabled() const noexcept override { return brewByTimeEnabled_; }
    double            getBrewByTimeTargetTime() const noexcept override { return brewByTimeTargetTime_; }
    bool              getBrewByWeightEnabled() const noexcept override { return brewByWeightEnabled_; }
    double            getBrewByWeightTargetWeight() const noexcept override { return brewByWeightTargetWeight_; }
    bool              getBrewPreInfusionEnabled() const noexcept override { return brewPreInfusionEnabled_; }
    double            getBrewPreInfusionTime() const noexcept override { return brewPreInfusionTime_; }
    double            getBrewPreInfusionPause() const noexcept override { return brewPreInfusionPause_; }
    Process::BrewMode getBrewMode() const noexcept override { return brewMode_; }

    // Hardware Switches
    bool                 getHardwareSwitchesBrewEnabled() const noexcept override { return hardwareSwitchesBrewEnabled_; }
    Hardware::SwitchType getHardwareSwitchesBrewType() const noexcept override { return hardwareSwitchesBrewType_; }
    bool                 getHardwareSwitchesSteamEnabled() const noexcept override { return hardwareSwitchesSteamEnabled_; }
    Hardware::SwitchType getHardwareSwitchesSteamType() const noexcept override { return hardwareSwitchesSteamType_; }
    bool                 getHardwareSwitchesPowerEnabled() const noexcept override { return hardwareSwitchesPowerEnabled_; }
    Hardware::SwitchType getHardwareSwitchesPowerType() const noexcept override { return hardwareSwitchesPowerType_; }
    bool                 getHardwareSwitchesHotWaterEnabled() const noexcept override { return hardwareSwitchesHotWaterEnabled_; }
    Hardware::SwitchType getHardwareSwitchesHotWaterType() const noexcept override { return hardwareSwitchesHotWaterType_; }

    // Hardware Sensors & Peripherals
    bool getHardwareSensorsScaleEnabled() const noexcept override { return hardwareSensorsScaleEnabled_; }
    bool getHardwareSensorsPressureEnabled() const noexcept override { return hardwareSensorsPressureEnabled_; }
    bool getHardwareOledEnabled() const noexcept override { return hardwareOledEnabled_; }

    // Hardware LEDs
    bool getHardwareLedsStatusEnabled() const noexcept override { return hardwareLedsStatusEnabled_; }
    bool getHardwareLedsBrewEnabled() const noexcept override { return hardwareLedsBrewEnabled_; }
    bool getHardwareLedsSteamEnabled() const noexcept override { return hardwareLedsSteamEnabled_; }

    // Standby
    bool   getStandbyEnabled() const noexcept override { return standbyEnabled_; }
    double getStandbyTime() const noexcept override { return standbyTime_; }

    // Backflush
    int    getBackflushCycles() const noexcept override { return backflushCycles_; }
    double getBackflushFillTime() const noexcept override { return backflushFillTime_; }
    double getBackflushFlushTime() const noexcept override { return backflushFlushTime_; }

    // MQTT
    bool getMqttEnabled() const noexcept override { return mqttEnabled_; }

    // Display
    System::DisplayTemplate getDisplayTemplate() const noexcept override { return displayTemplate_; }
    System::Language        getDisplayLanguage() const noexcept override { return displayLanguage_; }

    // ═══════════════════════════════════════════════════════════════
    // Convenience setters
    // ═══════════════════════════════════════════════════════════════

    void setPidEnabled(bool v) { pidEnabled_ = v; }
    void setPidUsePonm(bool v) { pidUsePonm_ = v; }
    void setPidRegularKp(double v) { pidRegularKp_ = v; }
    void setPidRegularTn(double v) { pidRegularTn_ = v; }
    void setPidRegularTv(double v) { pidRegularTv_ = v; }
    void setPidRegularIMax(double v) { pidRegularIMax_ = v; }
    void setPidSteamKp(double v) { pidSteamKp_ = v; }
    void setPidEmaFactor(double v) { pidEmaFactor_ = v; }
    void setPidBdEnabled(bool v) { pidBdEnabled_ = v; }
    void setPidBdKp(double v) { pidBdKp_ = v; }
    void setPidBdTn(double v) { pidBdTn_ = v; }
    void setPidBdTv(double v) { pidBdTv_ = v; }
    void setBrewPidDelay(double v) { brewPidDelay_ = v; }

    void setBrewSetpoint(double v) { brewSetpoint_ = v; }
    void setBrewTempOffset(double v) { brewTempOffset_ = v; }
    void setSteamSetpoint(double v) { steamSetpoint_ = v; }
    void setEmergencyStopTemp(double v) { emergencyStopTemp_ = v; }
    void setEmergencyStopHysteresis(double v) { emergencyStopHysteresis_ = v; }

    void setBrewByTimeEnabled(bool v) { brewByTimeEnabled_ = v; }
    void setBrewByTimeTargetTime(double v) { brewByTimeTargetTime_ = v; }
    void setBrewByWeightEnabled(bool v) { brewByWeightEnabled_ = v; }
    void setBrewByWeightTargetWeight(double v) { brewByWeightTargetWeight_ = v; }
    void setBrewPreInfusionEnabled(bool v) { brewPreInfusionEnabled_ = v; }
    void setBrewPreInfusionTime(double v) { brewPreInfusionTime_ = v; }
    void setBrewPreInfusionPause(double v) { brewPreInfusionPause_ = v; }
    void setBrewMode(Process::BrewMode v) { brewMode_ = v; }

    void setHardwareSwitchesBrewEnabled(bool v) { hardwareSwitchesBrewEnabled_ = v; }
    void setHardwareSwitchesBrewType(Hardware::SwitchType v) { hardwareSwitchesBrewType_ = v; }
    void setHardwareSwitchesSteamEnabled(bool v) { hardwareSwitchesSteamEnabled_ = v; }
    void setHardwareSwitchesSteamType(Hardware::SwitchType v) { hardwareSwitchesSteamType_ = v; }
    void setHardwareSwitchesPowerEnabled(bool v) { hardwareSwitchesPowerEnabled_ = v; }
    void setHardwareSwitchesPowerType(Hardware::SwitchType v) { hardwareSwitchesPowerType_ = v; }
    void setHardwareSwitchesHotWaterEnabled(bool v) { hardwareSwitchesHotWaterEnabled_ = v; }
    void setHardwareSwitchesHotWaterType(Hardware::SwitchType v) { hardwareSwitchesHotWaterType_ = v; }

    void setHardwareSensorsScaleEnabled(bool v) { hardwareSensorsScaleEnabled_ = v; }
    void setHardwareSensorsPressureEnabled(bool v) { hardwareSensorsPressureEnabled_ = v; }
    void setHardwareOledEnabled(bool v) { hardwareOledEnabled_ = v; }

    void setHardwareLedsStatusEnabled(bool v) { hardwareLedsStatusEnabled_ = v; }
    void setHardwareLedsBrewEnabled(bool v) { hardwareLedsBrewEnabled_ = v; }
    void setHardwareLedsSteamEnabled(bool v) { hardwareLedsSteamEnabled_ = v; }

    void setStandbyEnabled(bool v) { standbyEnabled_ = v; }
    void setStandbyTime(double v) { standbyTime_ = v; }

    void setBackflushCycles(int v) { backflushCycles_ = v; }
    void setBackflushFillTime(double v) { backflushFillTime_ = v; }
    void setBackflushFlushTime(double v) { backflushFlushTime_ = v; }

    void setMqttEnabled(bool v) { mqttEnabled_ = v; }

    void setDisplayTemplate(System::DisplayTemplate v) { displayTemplate_ = v; }
    void setDisplayLanguage(System::Language v) { displayLanguage_ = v; }

    void setSystemAuthEnabled(bool v) { systemAuthEnabled_ = v; }
    void setSystemAuthUsername(const String& v) { systemAuthUsername_ = v; }
    void setSystemAuthPassword(const String& v) { systemAuthPassword_ = v; }
};
