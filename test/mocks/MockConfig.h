/**
 * @file MockConfig.h
 * @brief Mock implementation of Config for testing
 * 
 * This provides a testable interface for Config that can be used in unit tests
 * without requiring actual NVS storage or hardware initialization.
 */

#pragma once

#include "clevercoffee/Config.h"
#include "clevercoffee/defaults.h"
#include <Arduino.h>

/**
 * @brief Mock Config implementation for testing
 * 
 * This class provides a minimal implementation of Config that can be used
 * in tests without requiring actual hardware or NVS storage.
 * 
 * Usage:
 * @code
 * MockConfig mockConfig;
 * mockConfig.setPidEnabled(true);
 * mockConfig.setBrewSetpoint(95.0);
 * 
 * // Use in test
 * ProcessController controller(mockConfig, ...);
 * @endcode
 */
class MockConfig {
public:
    MockConfig() = default;
    ~MockConfig() = default;

    // Configuration values with defaults
    bool pidEnabled_ = true;
    double brewSetpoint_ = 95.0;
    double steamSetpoint_ = 120.0;
    double pidRegularKp_ = 10.0;
    double pidRegularTn_ = 100.0;
    double pidRegularTv_ = 20.0;
    bool pidUsePonm_ = false;
    bool pidBdEnabled_ = false;
    double pidBdKp_ = 5.0;
    double pidBdTn_ = 50.0;
    double pidBdTv_ = 10.0;
    double brewPreInfusionTime_ = 5.0;
    double brewByTimeTargetTime_ = 30.0;
    bool standbyEnabled_ = true;
    double standbyTime_ = 30.0;
    bool hardwareSwitchesBrewEnabled_ = true;
    Hardware::SwitchType hardwareSwitchesBrewType_ = Hardware::SwitchType::MOMENTARY;
    bool hardwareSwitchesSteamEnabled_ = true;
    Hardware::SwitchType hardwareSwitchesSteamType_ = Hardware::SwitchType::MOMENTARY;
    bool hardwareSwitchesHotWaterEnabled_ = true;
    Hardware::SwitchType hardwareSwitchesHotWaterType_ = Hardware::SwitchType::MOMENTARY;
    bool hardwareSensorsScaleEnabled_ = true;
    bool hardwareOledEnabled_ = true;
    System::Language displayLanguage_ = System::Language::ENGLISH;
    bool systemAuthEnabled_ = false;
    String systemAuthUsername_ = "";
    String systemAuthPassword_ = "";
    double backflushFillTime_ = 5.0;
    double backflushFlushTime_ = 10.0;

    // Getters (matching Config interface pattern)
    bool getPidEnabled() const { return pidEnabled_; }
    double getBrewSetpoint() const { return brewSetpoint_; }
    double getSteamSetpoint() const { return steamSetpoint_; }
    double getPidRegularKp() const { return pidRegularKp_; }
    double getPidRegularTn() const { return pidRegularTn_; }
    double getPidRegularTv() const { return pidRegularTv_; }
    bool getPidUsePonm() const { return pidUsePonm_; }
    bool getPidBdEnabled() const { return pidBdEnabled_; }
    double getPidBdKp() const { return pidBdKp_; }
    double getPidBdTn() const { return pidBdTn_; }
    double getPidBdTv() const { return pidBdTv_; }
    double getBrewPreInfusionTime() const { return brewPreInfusionTime_; }
    double getBrewByTimeTargetTime() const { return brewByTimeTargetTime_; }
    bool getStandbyEnabled() const { return standbyEnabled_; }
    double getStandbyTime() const { return standbyTime_; }
    bool getHardwareSwitchesBrewEnabled() const { return hardwareSwitchesBrewEnabled_; }
    Hardware::SwitchType getHardwareSwitchesBrewType() const { return hardwareSwitchesBrewType_; }
    bool getHardwareSwitchesSteamEnabled() const { return hardwareSwitchesSteamEnabled_; }
    Hardware::SwitchType getHardwareSwitchesSteamType() const { return hardwareSwitchesSteamType_; }
    bool getHardwareSwitchesHotWaterEnabled() const { return hardwareSwitchesHotWaterEnabled_; }
    Hardware::SwitchType getHardwareSwitchesHotWaterType() const { return hardwareSwitchesHotWaterType_; }
    bool getHardwareSensorsScaleEnabled() const { return hardwareSensorsScaleEnabled_; }
    bool getHardwareOledEnabled() const { return hardwareOledEnabled_; }
    System::Language getDisplayLanguage() const { return displayLanguage_; }
    bool getSystemAuthEnabled() const { return systemAuthEnabled_; }
    String getSystemAuthUsername() const { return systemAuthUsername_; }
    String getSystemAuthPassword() const { return systemAuthPassword_; }
    double getBackflushFillTime() const { return backflushFillTime_; }
    double getBackflushFlushTime() const { return backflushFlushTime_; }

    // Setters
    void setPidEnabled(bool value) { pidEnabled_ = value; }
    void setBrewSetpoint(double value) { brewSetpoint_ = value; }
    void setSteamSetpoint(double value) { steamSetpoint_ = value; }
    void setPidRegularKp(double value) { pidRegularKp_ = value; }
    void setPidRegularTn(double value) { pidRegularTn_ = value; }
    void setPidRegularTv(double value) { pidRegularTv_ = value; }
    void setPidUsePonm(bool value) { pidUsePonm_ = value; }
    void setPidBdEnabled(bool value) { pidBdEnabled_ = value; }
    void setPidBdKp(double value) { pidBdKp_ = value; }
    void setPidBdTn(double value) { pidBdTn_ = value; }
    void setPidBdTv(double value) { pidBdTv_ = value; }
    void setBrewPreInfusionTime(double value) { brewPreInfusionTime_ = value; }
    void setBrewByTimeTargetTime(double value) { brewByTimeTargetTime_ = value; }
    void setStandbyEnabled(bool value) { standbyEnabled_ = value; }
    void setStandbyTime(double value) { standbyTime_ = value; }
    void setHardwareSwitchesBrewEnabled(bool value) { hardwareSwitchesBrewEnabled_ = value; }
    void setHardwareSwitchesBrewType(Hardware::SwitchType value) { hardwareSwitchesBrewType_ = value; }
    void setHardwareSwitchesSteamEnabled(bool value) { hardwareSwitchesSteamEnabled_ = value; }
    void setHardwareSwitchesSteamType(Hardware::SwitchType value) { hardwareSwitchesSteamType_ = value; }
    void setHardwareSwitchesHotWaterEnabled(bool value) { hardwareSwitchesHotWaterEnabled_ = value; }
    void setHardwareSwitchesHotWaterType(Hardware::SwitchType value) { hardwareSwitchesHotWaterType_ = value; }
    void setHardwareSensorsScaleEnabled(bool value) { hardwareSensorsScaleEnabled_ = value; }
    void setHardwareOledEnabled(bool value) { hardwareOledEnabled_ = value; }
    void setDisplayLanguage(System::Language value) { displayLanguage_ = value; }
    void setSystemAuthEnabled(bool value) { systemAuthEnabled_ = value; }
    void setSystemAuthUsername(const String& value) { systemAuthUsername_ = value; }
    void setSystemAuthPassword(const String& value) { systemAuthPassword_ = value; }
    void setBackflushFillTime(double value) { backflushFillTime_ = value; }
    void setBackflushFlushTime(double value) { backflushFlushTime_ = value; }
};
